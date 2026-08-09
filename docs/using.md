# Using the rule format

This document shows how to write a rule set, load it, run it, and read the
result. For what happens inside the loader, see
[architecture.md](architecture.md).

A rule set is one JSON file. Loading it gives you two directed creation
plans per rule, one per direction. You hand those to the engine together
with a graph.

Every example below is taken from the tree or was run before it was written
down. The file printed under [A worked example](#a-worked-example) loads,
lowers and runs as printed.

## The file, field by field

### Header

```json
{ "format": 3, "name": "uml_java", "rules": [] }
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `format` | number | yes | format version, must be `3` |
| `name` | string | no | name of the rule set, defaults to empty |
| `rules` | array | yes | the rules |

Unknown fields are rejected. There is no field the loader ignores.

### Rule

```json
{
  "name": "R_Class",
  "rank": 40,
  "documentation": "optional free text",
  "left":  { },
  "right": { },
  "corrs": [],
  "joins": []
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | unique within the file, enters identities |
| `rank` | number | yes | selection order, higher fires first |
| `documentation` | string | no | free text, carried through the loader |
| `left` | side | yes | one side of the rule |
| `right` | side | yes | the other side |
| `corrs` | array | no | correspondences between the sides |
| `joins` | array | no | cross side value equality constraints |

`joins` are pairs of node names, the first from the left side and the second
from the right. They are match constraints on value equality. They carry no
value.

### Side

```json
{
  "anchor": "cls",
  "nodes": [],
  "links": [],
  "same_value_links": []
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `anchor` | string | yes | fallback identity parent, see below |
| `nodes` | array | yes | the nodes of this side |
| `links` | array | no | directed pairs of node names |
| `same_value_links` | array | no | value equality constraints within this side |

Node names are local to their side. The left and the right side may use the
same name for different nodes.

`anchor` is required on both sides and is read in exactly one situation.
When the rule establishes no correspondence at all, created nodes hang off
the anchor of the input side. As soon as the rule has an `establishes`
correspondence, that correspondence carries the identity instead. Naming a
sensible node there is still worth it, because a rule without a
correspondence relies on it.

Links are pairs, directed, anonymous. There are no edge types. A role is a
node, not a label on an edge. The direction is creation provenance, meaning
the source owns the target.

Order matters twice. The order of `nodes` decides plan positions. The order
of `links` decides which link supplies the identity parent of a created
node, namely the first one that points at it.

### Node

```json
{ "name": "cname", "type": "name" }
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | unique within the side |
| `type` | string | yes | node type |
| `predicate` | object | no | value predicate, read while matching only |
| `context` | bool | no | matched, never created, carries no correspondence |
| `same_as` | string | no | right side only, names a node of the left side |
| `constant` | string | no | fixed value, written while creating only |

Attributes are leaf nodes whose type is the attribute name. There is no
attribute dictionary. A class with a name is a `Class` node linked to a
`name` node.

`predicate` and `constant` exclude each other in practice. A node is either
matched or created in a given direction, and the loader rejects the
combinations that would only work one way. See
[Errors you will hit](#errors-you-will-hit).

### Correspondence

```json
{
  "type": "CorrClass",
  "left": "cls",
  "right": "jcls",
  "role": "establishes",
  "bindings": []
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `type` | string | yes | type of the correspondence node |
| `left` | string | yes | node name on the left side |
| `right` | string | yes | node name on the right side |
| `role` | string | yes | `establishes` or `references` |
| `bindings` | array | no | value flow across this correspondence |

`role` is required. `establishes` means the rule creates this
correspondence and translates the element. `references` means the rule
requires an existing translation and matches it as context. The endpoint of
a `references` correspondence is never created.

### Binding

```json
{ "left": "cname", "right": "jname", "transform": [] }
```

A binding names a source and a target leaf and moves the value between them.
Two spellings exist and they exclude each other per binding.

- Static, by node name: `left` and `right`.
- Dynamic, by leaf type name: `left_type` and `right_type`. The source is
  looked up by type when the plan runs, at the input side endpoint of this
  correspondence. If it is absent, the leaf is skipped. This is
  apply-if-present.

Mixing one static and one dynamic side in a single binding has no meaning
and is rejected.

`transform` is a list of primitives applied in list order. It defaults to
the empty list, which is the identity.

| Primitive | JSON | Inverse |
|---|---|---|
| identity | `{"op":"identity"}` | itself |
| capitalize | `{"op":"capitalize"}` | decapitalize, conditional |
| decapitalize | `{"op":"decapitalize"}` | capitalize, conditional |
| prefix | `{"op":"prefix","arg":"get"}` | strip the prefix, fails if absent |
| suffix | `{"op":"suffix","arg":"Impl"}` | strip the suffix, fails if absent |

The backward direction is the element wise inverse in reverse order. It is
applied like any other chain, so a strip that finds no matching affix yields
no value at all. The two case operations have no such failure case. They are
applied without a check, and on values like `URL` or `ß` the round trip does
not return the original.

## Loading and lowering

```rust
use seesaw_tgg::graph::Graph;

let mut g = Graph::default();
let lowered = seesaw_tgg::rules::load(source, &mut g).expect("rule file loads");
```

`rules::load` is the one way, and it runs three stages: parse, validate,
lower. Its error says which stage failed:

```rust
use seesaw_tgg::rules::LoadError;

match seesaw_tgg::rules::load(source, &mut g) {
    Ok(rules) => { /* … */ }
    Err(LoadError::Parse(e)) => eprintln!("not the JSON this format expects: {e}"),
    Err(LoadError::Validate(e)) => eprintln!("the file says something inconsistent: {e:?}"),
    Err(LoadError::Lower(e)) => eprintln!("consistent, but not lowerable: {e:?}"),
}
```

Three points about this.

If you need the stages separately, `rules::format::RuleFile::from_json`,
`rules::validate::validate` and `rules::lower::lower_all` are public.
`load_file` takes an already parsed file, for hosts that build it
themselves instead of reading text.

`validate` returns a `Resolved` that owns the interning table for transform
chains. Lowering takes the whole `Resolved` and never a single rule out of
it, because a chain id only means something in its own table.

Loading needs the graph, because it interns type names into that graph's
type table. Use the same graph instance for the seed data afterwards. A rule
set lowered against one graph does not fit another.

The result holds two directed rules per rule of the file, forward first, in
declaration order. Their names carry the direction.

```rust
assert_eq!(lowered.len(), 2, "one rule, two directions");
assert_eq!(lowered[0].name, "R_Class→");
assert_eq!(lowered[1].name, "R_Class←");
```

## Running a cascade

The file used here is `tests/fixtures/v3/uml_java_min.json`, the single rule
`R_Class`, and the run is the one in `tests/v3_format.rs`.

```json
{
  "format": 3,
  "name": "uml_java_min",
  "rules": [
    {
      "name": "R_Class",
      "rank": 40,
      "left": {
        "anchor": "cls",
        "nodes": [
          { "name": "model", "type": "Model" },
          { "name": "cls", "type": "Class" },
          { "name": "cname", "type": "name" }
        ],
        "links": [["model", "cls"], ["cls", "cname"]]
      },
      "right": {
        "anchor": "jcls",
        "nodes": [
          { "name": "jmodel", "type": "Model", "same_as": "model" },
          { "name": "jcls", "type": "JavaClass" },
          { "name": "jname", "type": "name" }
        ],
        "links": [["jmodel", "jcls"], ["jcls", "jname"]]
      },
      "corrs": [
        {
          "type": "CorrClass",
          "left": "cls",
          "right": "jcls",
          "role": "establishes",
          "bindings": [{ "left": "cname", "right": "jname", "transform": [] }]
        }
      ]
    }
  ]
}
```

```rust
use seesaw_tgg::engine::{Engine, Termination};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;

let mut g = Graph::default();
let lowered = seesaw_tgg::rules::load(MIN, &mut g).expect("rule file loads");

// Seed: Model -> Class -> name leaf.
let model = g.add_baseline("m", "Model");
let cls = g.add_baseline("m/Person", "Class");
let cname = g.add_baseline("m/Person/name", "name");
g.connect(model, cls, Status::Solid);
g.connect(cls, cname, Status::Solid);

let mut vs = ValueStore::default();
vs.insert(cname, "Person");

let mut engine = Engine::new(&lowered);
let verdict = engine.run(&mut g, &vs, 1000);
assert!(matches!(
    verdict,
    Termination::Convergence | Termination::Duplication
));
```

`add_baseline` takes a stable external name from the host and a type. That
name is the only identity input that comes from outside. Everything the
cascade creates derives its identity structurally.

Values live outside the graph. `ValueStore` is the standalone resolver for
tests and benchmarks. A host adapter implements the same `ValueResolver`
trait over its own model.

The third argument to `run` is a step budget, not a target. It exists so a
non terminating rule set stops.

`Termination` has four values.

| Value | Meaning |
|---|---|
| `Convergence` | nothing was applied at all, the candidate list ran dry |
| `Duplication` | saturation after at least one application or duplicate hit |
| `StepLimit` | the budget ran out, the run is unfinished |
| `Contradiction` | a candidate wanted to reuse tombstoned substance |

The first two are both regular termination. Treat a `StepLimit` as an
unfinished run, not as a result.

Running the same rules again on the same graph with a fresh engine creates
nothing new. Convergence comes from the identities in the graph, not from
the memory of one engine instance.

## Reading the result

The graph is an overlay. Reading means filtering by status.

```rust
// Live nodes of one type.
fn count_of_type(g: &Graph, typ: &str) -> usize {
    g.types
        .lookup(typ)
        .map(|t| g.nodes_of_type(t).filter(|n| n.status.is_matchable()).count())
        .unwrap_or(0)
}

assert_eq!(count_of_type(&g, "JavaClass"), 1);
assert_eq!(count_of_type(&g, "CorrClass"), 1);
```

`nodes_of_type` already filters for matchable status. The extra filter in
the helper is harmless and makes the intent visible at the call site.

Leaf values are resolved, not stored. `resolve_value` walks the derivation
down to a baseline leaf or a constant and applies the transformations
forward.

```rust
let jcls = {
    let t = g.types.lookup("JavaClass").expect("type exists");
    g.nodes_of_type(t)
        .find(|n| n.status.is_matchable())
        .expect("one JavaClass")
        .id
};
let jname = g
    .child_leaf_of_type(&jcls, "name")
    .expect("JavaClass has a name leaf");
assert_eq!(g.resolve_value(&jname, &vs).as_deref(), Some("Person"));
```

`materialize` ends the overlay. It returns a new graph without tombstones
where every ghost is `Solid`. Connections survive only if both endpoints
survive. Values stay where they were, so resolution works exactly as before.

```rust
let solid = g.materialize();
assert_eq!(count_of_type(&solid, "JavaClass"), 1);
assert_eq!(solid.resolve_value(&jname, &vs).as_deref(), Some("Person"));
```

## A worked example

Two rules, a `references` correspondence, a constant, and a cascade that
runs both rules in order. This is the whole file.

```json
{
  "format": 3,
  "name": "uml_java",
  "rules": [
    {
      "name": "R_Class",
      "rank": 40,
      "left": {
        "anchor": "cls",
        "nodes": [
          { "name": "model", "type": "Model" },
          { "name": "cls", "type": "Class" },
          { "name": "cname", "type": "name" }
        ],
        "links": [["model", "cls"], ["cls", "cname"]]
      },
      "right": {
        "anchor": "jcls",
        "nodes": [
          { "name": "jmodel", "type": "Model", "same_as": "model" },
          { "name": "jcls", "type": "JavaClass" },
          { "name": "jname", "type": "name" }
        ],
        "links": [["jmodel", "jcls"], ["jcls", "jname"]]
      },
      "corrs": [
        {
          "type": "CorrClass",
          "left": "cls",
          "right": "jcls",
          "role": "establishes",
          "bindings": [{ "left": "cname", "right": "jname", "transform": [] }]
        }
      ]
    },
    {
      "name": "R_Attr",
      "rank": 30,
      "left": {
        "anchor": "attr",
        "nodes": [
          { "name": "cls", "type": "Class" },
          { "name": "attr", "type": "Attribute" },
          { "name": "aname", "type": "name" }
        ],
        "links": [["cls", "attr"], ["attr", "aname"]]
      },
      "right": {
        "anchor": "field",
        "nodes": [
          { "name": "jcls", "type": "JavaClass" },
          { "name": "field", "type": "JavaField" },
          { "name": "fname", "type": "name" },
          { "name": "vis", "type": "visibility", "constant": "private" }
        ],
        "links": [["jcls", "field"], ["field", "fname"], ["field", "vis"]]
      },
      "corrs": [
        {
          "type": "CorrClass",
          "left": "cls",
          "right": "jcls",
          "role": "references"
        },
        {
          "type": "CorrAttr",
          "left": "attr",
          "right": "field",
          "role": "establishes",
          "bindings": [{ "left": "aname", "right": "fname", "transform": [] }]
        }
      ]
    }
  ]
}
```

Read the second rule from its correspondences. `CorrClass` with role
`references` says that the class must already be translated, and it does not
create anything. `CorrAttr` with role `establishes` says that this rule
translates the attribute and creates the correspondence for it. `jcls` is
therefore matched, `field` is created, and the link `["jcls", "field"]` hangs
the new field off the existing Java class.

`vis` carries a constant. It is created in the forward direction, matched in
the backward one. The value `private` lives in the rule. The identity of
that leaf does not depend on it.

Lowering yields four directed rules.

```rust
let lowered = seesaw_tgg::rules::load(source, &mut g).expect("rule file loads");
let names: Vec<&str> = lowered.iter().map(|r| r.name.as_str()).collect();
assert_eq!(names, vec!["R_Class→", "R_Class←", "R_Attr→", "R_Attr←"]);
```

Seed a model with one class and one attribute.

```rust
let model = g.add_baseline("m", "Model");
let cls = g.add_baseline("m/Person", "Class");
let cname = g.add_baseline("m/Person/name", "name");
let attr = g.add_baseline("m/Person/age", "Attribute");
let aname = g.add_baseline("m/Person/age/name", "name");
g.connect(model, cls, Status::Solid);
g.connect(cls, cname, Status::Solid);
g.connect(cls, attr, Status::Solid);
g.connect(attr, aname, Status::Solid);

let mut vs = ValueStore::default();
vs.insert(cname, "Person");
vs.insert(aname, "age");

let mut engine = Engine::new(&lowered);
let verdict = engine.run(&mut g, &vs, 10_000);
```

What happens, in order. `R_Class` has the higher rank, so it fires first and
creates `CorrClass`, the `JavaClass` and its name leaf. That makes the
`references` precondition of `R_Attr` true, so `R_Attr` fires next and
creates `CorrAttr`, the `JavaField`, its name leaf and the constant
`visibility` leaf. Then the run saturates.

The backward directions do not undo any of it. Both of them find an
established correspondence of their type at the anchor, with a live endpoint
of the expected type, so the elements count as translated and nothing is
created a second time.

```rust
assert_eq!(count_of_type(&g, "JavaClass"), 1);
assert_eq!(count_of_type(&g, "CorrClass"), 1);
assert_eq!(count_of_type(&g, "JavaField"), 1);
assert_eq!(count_of_type(&g, "CorrAttr"), 1);
assert_eq!(count_of_type(&g, "Attribute"), 1, "no second Attribute");
assert_eq!(count_of_type(&g, "Class"), 1, "no second Class");

let field = {
    let t = g.types.lookup("JavaField").expect("type exists");
    g.nodes_of_type(t)
        .find(|n| n.status.is_matchable())
        .expect("one JavaField")
        .id
};
let fname = g.child_leaf_of_type(&field, "name").expect("name leaf");
assert_eq!(g.resolve_value(&fname, &vs).as_deref(), Some("age"));
let vis = g
    .child_leaf_of_type(&field, "visibility")
    .expect("visibility leaf");
assert_eq!(g.resolve_value(&vis, &vs).as_deref(), Some("private"));
```

## Transformations in both directions

A binding with a chain, run forward and backward.

```json
{
  "left": "aname",
  "right": "mname",
  "transform": [{ "op": "capitalize" }, { "op": "prefix", "arg": "get" }]
}
```

Forward, an `Attribute` named `age` produces a `Method` whose name leaf
resolves to `getAge`. Backward, a `Method` named `getAge` produces an
`Attribute` whose name leaf resolves to `age`. The backward chain is the
element wise inverse in reverse order, so `prefix("get")` becomes a strip
and `capitalize` becomes `decapitalize`.

```rust
assert_eq!(g.resolve_value(&mname, &vs).as_deref(), Some("getAge"));
// ... and on a graph seeded from the Java side:
assert_eq!(g2.resolve_value(&an2, &vs2).as_deref(), Some("age"));
```

Backward can fail, and failing is the correct outcome. A method named
`setAge` yields no value through this chain, because the prefix is not
there. If you need the stricter test, whether a target value is reachable
through the forward chain at all, call `Chain::invert_checked` yourself.
Value resolution does not.

## Writing values back: the host checks reachability

The backward direction installs the inverse chain and applies it forward.
It does not verify that the value it computes maps back to the target you
asked for. That check belongs to whoever accepts a changed value, which is
the host adapter, not the library.

Take the getter chain: capitalize, then prefix `get`. From `age` it makes
`getAge`. If a host accepts `getage` from its editor and hands it to the
backward direction, the result is `age`, which maps forward to `getAge`
again. The value the user typed is silently replaced by a different one.

`Chain::invert_checked` exists for this. It computes the source and
returns it only if the forward chain maps it back to the target:

```rust
match chain.invert_checked(target) {
    Some(source) => accept(source),
    None => reject(target),  // not producible by this rule
}
```

A host that skips this check will correct user input without saying so.
Strip operations fail on their own when the affix is missing. Case
operations do not, so chains containing capitalize or decapitalize are
the ones where it matters.

## Value predicates

Five kinds, all optional on a node.

| Kind | JSON | Matches |
|---|---|---|
| exists | `{"kind":"exists"}` | any node that has a value |
| equals | `{"kind":"equals","value":"yes"}` | exactly that string |
| prefix | `{"kind":"prefix","value":"get"}` | values starting with it |
| regex | `{"kind":"regex","pattern":"Abstract.*"}` | full match against the pattern |
| numeric_range | `{"kind":"numeric_range","min":0.0,"max":100.0}` | numbers inside, bounds included |

A predicate is only read while matching. Put it on a node that the rule
matches rather than creates, which means a `context` node, a `same_as`
partner, or the endpoint of a `references` correspondence. On a created node
it is a load error. The examples in `tests/rules_attr_regex_v3.rs` use a
context leaf on the input side.

```json
{ "name": "attr", "type": "Attr", "context": true,
  "predicate": { "kind": "regex", "pattern": "Abstract.*" } }
```

### Four points the format normalizes

Two languages evaluate these predicates, so four points are fixed by the
format rather than left to a regex dialect.

**Full match, not partial.** Rust evaluates `\A(?:pattern)\z`, Java uses
`Matcher.matches()`. The pattern `ab` matches the value `ab` and does not
match `xaby`. If you want a partial match, write it out, for example
`Abstract.*` instead of a prefix pattern.

**No anchors in the pattern.** `^` and `$` are rejected. They are redundant
under a full match and they differ between dialects, since Java's `$`
matches before a final line break and Rust's does not. The check is context
sensitive. `^` inside a character class, as in `[^a]`, is negation and stays
allowed, and an escaped `\^` is a literal and stays allowed too.

**A restricted syntax subset.** Allowed are literals, escapes, character
classes, `.`, alternation, groups and the quantifiers `*`, `+`, `?` and
`{n,m}`. Rejected are lookaround, backreferences, named groups in both
spellings, possessive quantifiers, `\b`, `\B`, `\p{...}` and `\P{...}`. The
loader checks the subset and rejects what is outside it.

**A number grammar of its own.** A value is a number if it matches an
optional sign, digits, an optional decimal point with digits, and an
optional exponent. No hex floats, no `d` or `f` suffixes, no `inf`, no
`NaN`. `-1.5e3` is a number. `1d`, `0x1p3`, `inf` and `NaN` are not, and a
node carrying one of those does not match a `numeric_range`. Both bounds are
inclusive.

## Three things that go into identity

Identity is derived from provenance, never from a value. Three inputs come
from places that do not look like identity while writing a rule set.

**The rule name** enters the identity of every constant leaf the rule
creates. Renaming a rule moves those leaves. Rule names must be unique
within a file, and the loader rejects duplicates.

**The direction suffix** is appended by lowering, `→` for forward and `←`
for backward, and it is part of the name that enters the constant identity.
The rule `R_Attr` produces `R_Attr→` and `R_Attr←`.

**The declaration order of `nodes`** decides plan positions, and the plan
index enters the constant identity. The order of `links` decides which link
supplies the identity parent of a created node, namely the first one that
points at it. A generator that sorts these lists differently moves
identities. The format looks name based and is order dependent at this
point.

## Errors you will hit

Two kinds of error exist. A malformed file fails in `RuleFile::from_json` as
a serde error with a line and column. A file that parses but does not hold
together fails in `validate` as a `LoadError`.

`LoadError` implements `Debug`, not `Display`. Format it with `{:?}`. The
right hand column below is the actual output.

### While parsing

| Cause | Message |
|---|---|
| a field the format does not know | ``unknown field `nacs`, expected one of `format`, `name`, `rules` at line 1 column 40`` |
| a correspondence without `role` | ``missing field `role` at line 4 column 56`` |
| an unknown transform operation | ``unknown variant `reverse`, expected one of `identity`, `capitalize`, `decapitalize`, `prefix`, `suffix` at line 5 column 78`` |

### While validating

| Cause | `{:?}` output |
|---|---|
| `"format": 2` | `Version { found: 2, expected: 3 }` |
| two rules named the same | `DuplicateRuleName { name: "R_Class" }` |
| two nodes named the same on one side | `DuplicateNode { rule: "R_Class", side: "left", name: "cls" }` |
| a typo in a link, join or correspondence | `UnknownNode { rule: "R_Class", side: "left", name: "typo" }` |
| an anchor that names no node | `UnknownAnchor { rule: "R_Class", side: "left", name: "klass" }` |
| the same link twice on one side | `DuplicateLink { rule: "R_Class", side: "left", a: "model", b: "cls" }` |
| `same_as` on the left side | `SameAsOnLeft { rule: "R_Class", name: "cname" }` |
| `same_as` pointing at no left node | `UnknownSameAs { rule: "R_Class", name: "modell" }` |
| a binding with a node and a type source on one side | `AmbiguousBinding { rule: "R_Class", corr: "CorrClass" }` |
| a binding static on one side and dynamic on the other | `MixedBinding { rule: "R_Class", corr: "CorrClass" }` |
| a binding with no source at all | `EmptyBinding { rule: "R_Class", corr: "CorrClass" }` |
| a predicate on a node the rule creates | `PredicateOnCreatedNode { rule: "R_Class", side: "left", node: "cname" }` |
| equality predicate and constant disagreeing | `ConstantPredicateMismatch { rule: "R_Class", side: "left", node: "cname" }` |
| a constant on a node that is never created | `ConstantOnMatchedNode { rule: "R_Class", side: "right", node: "jmodel" }` |
| an anchor in a regex pattern | `Predicate { rule: "R_Class", node: "cname", err: ForbiddenSyntax("^") }` |
| a broken regex pattern | `Predicate { rule: "R_Class", node: "cname", err: BadRegex("regex parse error:\n    \\A(?:()\\z\n      ^\nerror: unclosed group") }` |

`DuplicateSameValueLink` exists as well, with the same shape as
`DuplicateLink`, for a repeated pair in `same_value_links`.

### Reading the three role errors

These three are the ones that catch beginners, so here is what they mean.

`PredicateOnCreatedNode` says that the node carries a predicate but the rule
creates it in one of the two directions. A predicate is never read while
creating, so the rule would only hold half the time. The fix is either to
mark the node `context: true`, or to move the predicate to a node the rule
actually matches. One exception is allowed, an equality predicate whose
value equals the `constant` of the same node.

`ConstantOnMatchedNode` says the opposite. The node carries a constant but
is never created, because it is `context`, or a `same_as` partner, or the
endpoint of a `references` correspondence. The constant would fall through
in both directions.

`ConstantPredicateMismatch` says that both are present and disagree. The
matching direction wants one value and the creating direction writes
another.

## What is not there

- No negative application conditions. There is no field for them, and a
  file carrying one is rejected while parsing. Express the condition as
  positive structure instead.
- No converter for older rule formats, and no way to read them.
- No JSON schema for editor support.
- No hot reload beyond loading a file again.
- No rule editor.
- No `Display` on the error types. `Debug` only.
- No multigraph. At most one connection exists per source and target pair.
- No guaranteed invertibility of `capitalize` and `decapitalize`. The
  backward direction applies them without checking the result.
  `Chain::invert_checked` performs that check and is not called anywhere
  inside the library.
