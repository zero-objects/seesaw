# Architecture

This document describes how a rule set travels from a file on disk to a
running cascade, and where identity comes from. It is written for readers
who want to know what happens inside, not how to write a rule set. That is
[using.md](using.md).

The rule format is version 3. A file declares rules by name. Loading a file
produces two directed creation plans per rule, one for each direction. The
engine underneath executes those plans. The engine itself is not documented
here, only the places where it becomes visible.

## The path of a rule file

```
uml_java.json
     │  serde, deny_unknown_fields
     ▼
RuleFile        rules::format      the file, one struct per JSON object
     │  validate()
     ▼
Resolved        rules::validate    names resolved to positions, checks done
     │  lower_all()
     ▼
Vec<DirectedRule>               two per rule, forward and backward
     │  Engine::new(&rules).run(&mut graph, &values, budget)
     ▼
Graph                           ghost overlay over the host model
```

Four modules carry the four stages.

| Module | File | Responsibility |
|---|---|---|
| `rules::format` | `src/rules/format.rs` | file structures, serde only, no logic |
| `rules::validate` | `src/rules/validate.rs` | validation, name resolution, chain interning |
| `rules::lower` | `src/rules/lower.rs` | two directed creation plans per rule |
| `rules::export` | `src/rules/export.rs` | serializes lowered plans, verification artifact |

`rules::predicate` and `rules::transform` are used by two stages each. Predicates
are parsed during validation and evaluated during matching. Transform chains
are interned during validation, inverted during lowering, and applied during
value resolution.

## Stage 1: the file

`RuleFile::from_json` is a plain serde deserialization. It has no knowledge
of rules beyond their shape. Everything it can reject, it rejects at this
point.

Every struct of the format carries `#[serde(deny_unknown_fields)]`. A field
that does not belong to the format is an error, not a value that gets
ignored. A file that carries a field from an earlier generation fails to
parse instead of loading with that part missing.

```
unknown field `nacs`, expected one of `format`, `name`, `rules` at line 1 column 40
```

The two internally tagged enums, `PrimDecl` (tag `op`) and `PredicateDecl`
(tag `kind`), cannot rely on `deny_unknown_fields` on the enum itself.
Serde buffers the object before it reads the tag, and a foreign field is
lost in that buffer. Each variant therefore carries its body as a separate
named struct, which is strict. `{"op":"prefix","arg":"get","unknown":true}`
fails. The JSON shape stays flat either way.

Both enums are closed. An operation or predicate kind the format does not
know is a parse error with the list of valid names in the message.

```
unknown variant `reverse`, expected one of `identity`, `capitalize`, `decapitalize`, `prefix`, `suffix`
```

`format` is checked in the next stage, not here. A file with a foreign
version number still parses if it happens to fit the structures. It is
rejected by `validate`.

## Stage 2: validation and name resolution

`validate(&RuleFile) -> Result<Resolved, LoadError>` does two things at once.
It checks the rules, and it turns every name into a position.

Names are file-local and per side. `left` and `right` have separate name
spaces. The index for each side is built once, in `side_index`, and then
used for the anchor, for the links, for `same_as` targets, for correspondence
endpoints and for bindings. A duplicate node name inside one side is an error
before anything is resolved against it.

After resolution, the `Resolved*` structures hold positions where the file
held names.

| File | Resolved |
|---|---|
| `SideDecl.anchor: String` | `ResolvedSide.anchor: usize` |
| `SideDecl.links: Vec<(String, String)>` | `ResolvedSide.links: Vec<(usize, usize)>` |
| `NodeDecl.same_as: Option<String>` | `ResolvedNode.same_as: Option<usize>` |
| `CorrDecl.left/right: String` | `ResolvedCorr.left/right: usize` |
| `BindingDecl.left` or `left_type` | `BindingSource::Node(usize)` or `LeafType(String)` |
| `BindingDecl.transform: Vec<PrimDecl>` | `ResolvedBinding.chain: ChainId` |

The chain table lives in the result, as `Resolved.chains`, not beside it. A
`ChainId` resolved against a different table gives a panic in the good case
and the wrong chain in the bad one. Lowering therefore takes the whole
`Resolved` and an index, never a single rule.

Every error carries its location. `UnknownNode` names the rule, the side and
the name that was not found. For a cross-side join, the first name is
resolved against the left index and the second against the right one, so a
name that exists but sits on the wrong side is reported on the side it was
looked for.

### Rule names must be unique

Duplicate rule names inside one set are a load error, checked before any
rule is resolved. This is not a style rule. The rule name enters the
identity of every constant the rule creates. Two rules with the same name
and a constant at the same plan position under the same parent would produce
the same identity for two different elements. See the identity section below.

### Predicate and constant must match the node role

A value predicate is only read while matching. A constant is only written
while creating. Whether a node is matched or created is not declared, it
follows from the rule.

`is_created(rule, side, i)` answers the question for the one direction in
which that side is the output side. A node is created there unless it is one
of three things.

1. `context: true`. The node is matched, never created.
2. The partner of a `same_as` relation. The right node is a left node, so
   neither of them is created on the output side.
3. The endpoint of a correspondence with `role: "references"`. The rule
   points at an existing translation instead of establishing one.

From that, `check_value_roles` derives four errors.

- A predicate on a node that lowering creates is `PredicateOnCreatedNode`.
  Exactly one form is allowed, an equality predicate whose value equals the
  `constant` of the same node. That node is matched by value in one
  direction and written with that same value in the other.
- The same node with an equality predicate and a different constant is
  `ConstantPredicateMismatch`.
- A constant on a node that is never created in either direction is
  `ConstantOnMatchedNode`. It would fall through both ways.
- An invalid regular expression or forbidden regex syntax is `Predicate`,
  carrying the rule, the node and a `PredicateError`.

The point of these four is that a rule which only holds in one direction is
rejected at load time instead of running half correctly.

### Transform chains are normalized on the way in

`ChainTable::intern` stores the normal form of a chain, and returns the same
id for two chains that only differ in ways the normal form removes. Three
rules apply, each of them effect preserving.

1. `identity` steps drop out.
2. Affix steps with an empty argument drop out.
3. Adjacent affix steps of the same kind merge.

The normal form is not the shortest chain with the same effect. Shortenings
that depend on Unicode edge cases are deliberately not applied.
`[capitalize, capitalize]` stays as written, because idempotence of
`capitalize` fails for values like `ß`. The reason for normalizing at all is
identity, not size. The chain enters the identity of every leaf derived
through it, so two rules that write the same transformation differently must
not produce two different leaves.

## Stage 3: lowering

`lower_rule(&Resolved, ix, &mut Graph)` returns `[DirectedRule; 2]`, forward
first. `lower_all` does the same for the whole set and returns them
alternating in declaration order. Both take a graph, because pattern node
types are interned into that graph's type table. The lowered rules only fit
the graph they were lowered against.

Lowering is a mirror. One side becomes a pattern, the other becomes a
creation plan. Forward means left matches and right is created. Backward
swaps the two. Everything below is written for one direction, with `inn` for
the input side and `out` for the output side.

### The pattern

The input side becomes the pattern, one to one. Position `i` of the input
side is position `i` of the pattern. Nodes carry the interned type and their
predicate. Links become directed pattern links.

The pattern then grows at the end, never in the middle, so input positions
stay stable.

- Output nodes that are matched instead of created are appended. Those are
  `same_as` partners, `context` nodes, and the endpoints of `references`
  correspondences. The table `out_ctx_pattern_pos` maps an output position
  to the pattern position where it landed.
- A `references` correspondence appends two nodes, the correspondence itself
  and its output endpoint, joined by two direction free context links. The
  correspondence hangs off its own endpoint on the input side.
- Output links between two matched nodes become pattern links. That is a
  precondition, not something to create.

One exception to that last point. A link between two `references` endpoints
is not a precondition. Two independently referenced existences say nothing
about their adjacency, and creating that edge is the purpose of such a rule.
Every link that touches a `context` or `same_as` node stays a precondition,
because its attachment is the author's existence statement.

Value equality constraints reach the pattern from three places. Input side
`same_value_links` go in unadjusted. Output side `same_value_links` go in
only if both ends are matched. A cross side `join` goes in if the output end
is matched. A join on a created node is no constraint at all, because the
value only comes into being there.

### The creation plan

The plan is a list of `CreateNode` entries plus a list of links between
them. Links refer to plan entries by `Ref::New(ix)` and to pattern positions
by `Ref::Matched(pos)`.

Correspondences with `role: "establishes"` come first, one plan entry each.
A rule may have several. Each hangs off its own endpoint on the input side
and gets a link from it. If the established endpoint is itself matched, its
reference enters the correspondence identity, otherwise several matches at
the same anchor would collapse into one.

Then every output position that is not matched becomes a plan entry, in
declaration order. Each entry carries at most one value source.

- A static binding makes it a derived leaf. The value comes from an input
  leaf through a chain. In the backward direction the chain is the inverse.
- A `constant` makes it a constant leaf. The value lives in the rule.
- Both at once is a lowering error.
- Neither makes it an ordinary ghost node.

Dynamic bindings, declared with `left_type` and `right_type` instead of node
names, are appended afterwards, one plan entry per binding, hanging off the
established endpoint. Their source is looked up by leaf type when the plan
runs, at the input side endpoint of their correspondence. If the source leaf
is absent, the leaf is skipped. This is the apply-if-present rule.

Finally the output side's links become plan links, and each establishing
correspondence gets a link to the endpoint it establishes.

### The identity parent

Every created node needs a parent, because identity is derived from it. The
precedence is fixed and worth knowing.

1. The correspondence that establishes this very position, if there is one.
2. The created structural parent, taken from the output side's links. The
   parent of a node is the source of the first link that points at it.
3. The first establishing correspondence of the rule.
4. The input anchor.

Step 4 is the only place where the declared `anchor` of a side is read.
Everything else that lowering calls an anchor is a correspondence endpoint,
taken from `corrs`, not from the `anchor` field. A rule with at least one
establishing correspondence never reaches step 4.

Matched nodes never qualify as an identity parent. Two matches at the same
context node would otherwise produce colliding siblings.

Note what step 2 means. The parent comes from the first link in the declared
order that points at the node. Reordering `links` can move identities.

### What else the directed rule carries

`input_types` is the list of type names that make this direction relevant
for a delta. It contains the input side's types, all correspondence types,
and the types of matched output nodes. A rule whose only new trigger is the
correspondence itself has to fire once that correspondence appears, which is
why correspondence types are in the list.

`corr_recognition` holds one triple per establishing correspondence, made of
the correspondence type, the pattern position of its input side endpoint,
and the type of the established endpoint. The engine uses it to recognize
that an element has already been translated. This is what keeps the opposite
direction from translating a translation back into a duplicate.

`name` is the rule name plus a direction suffix. Forward gets `→`, U+2192.
Backward gets `←`, U+2190. `R_Class` becomes `R_Class→` and `R_Class←`.

## Stage 4: the engine, where it shows

The engine takes a slice of directed rules and a graph. What a rule author
sees of it is this.

Matching is positional. A match is the sequence of node identities in
pattern order, and that sequence is the match key. There are no variable
names below the format level.

Candidates are ordered by rank descending, then by the reference sequence
descending, then by rule index ascending. Higher `rank` fires first.

Before a candidate is applied, the engine checks whether it would produce
anything new. It can do that without applying, because identities are pure
functions of provenance. It also checks `corr_recognition`. If the anchor
already carries a correspondence of that type whose other endpoint is alive
and of the expected type, the element counts as translated and nothing
happens, no matter which direction or which rule variant produced it.

`Engine::run` returns a `Termination`.

| Value | Meaning |
|---|---|
| `Convergence` | the candidate list ran dry without a single application |
| `Duplication` | saturation after at least one application or duplicate hit |
| `StepLimit` | the step budget was used up, the run is not finished |
| `Contradiction` | a candidate wanted to reuse tombstoned substance |

`Convergence` and `Duplication` are both regular termination. The name of
the second one describes how saturation is detected, not a problem.

## Identity

This is the central promise of the library. Identity is derived structurally
from provenance. A model value never enters an identity.

An identity is a 32 byte blake3 hash. Six derivations exist, each with its
own domain tag, so hashes from different kinds can never collide by
construction.

| Kind | Tag | Hashed input |
|---|---|---|
| baseline | `V2B` | the external name given by the host |
| ghost node | `V2G` | parent id, type name |
| derived leaf | `V2D` | parent id, type name, source leaf id, chain bytes |
| connection | `V2C` | source id, target id |
| correspondence | `V2R` | anchor id, type name, the match reference sequence |
| constant leaf | `V2K` | parent id, type name, rule name, plan index |

Read the constant row again. A constant leaf carries a value, and that value
is nowhere in its identity. Two rule variants that write different constants
at the same position under the same parent produce two structurally
different leaves. Changing the constant of a rule does not move the leaf it
creates. The same holds for a derived leaf. What enters there is the chain,
in its normal form, including its arguments, because the chain belongs to
the rule. The source value does not.

### Which derivation a correspondence node gets

The `V2R` row is the graph's match digest derivation. Rules lowered from
this format never reach it, because the plan flag that selects it,
`CreateNode.corr_full_match`, is always `false` here. A correspondence node
created by a v3 rule gets one of two identities instead, and which one
depends on the established endpoint.

- The endpoint is created by the same rule. The correspondence is an
  ordinary ghost node, `V2G` over the input side endpoint of the
  correspondence and the correspondence type.
- The endpoint is matched, because it is `context` or a `same_as` partner.
  The correspondence is a derived node, `V2D` over the same parent, the
  correspondence type, the identity of the matched counterpart, and the
  empty chain.

The second form carries a discriminator, which is the point of it. Without
the counterpart's identity, several matches at one anchor would collapse
into a single correspondence.

The first form carries none. Its identity is the pair of anchor and
correspondence type and nothing else. Two rules that establish the same
correspondence type at the same anchor, each creating its own endpoint,
therefore produce one correspondence node, not two. Give distinct
correspondences distinct types.

### Three things you would not guess

These three enter the identity and are not visible when reading a rule file
as if it were a picture.

**The rule name.** It is hashed into every constant leaf the rule creates.
Renaming a rule changes those identities. Two rules of one set may not share
a name, and the loader rejects the file if they do.

**The direction suffix.** Lowering appends `→` for the forward direction and
`←` for the backward one, three bytes each in UTF-8. That name is what goes
into the constant identity, not the name in the file. A second loader must
use exactly these two characters.

**The declaration order of `nodes`.** The order in `nodes` decides plan
positions, and the plan index is hashed into constant identities. It also
decides which link supplies the identity parent of a created node, namely
the first one pointing at it. A generator that sorts `nodes` differently
moves identities. The format reads as name based and is order dependent at
this one point.

## Three states, retraction, materialization

Every node and every connection carries a `Status`.

| State | Meaning |
|---|---|
| `Solid` | baseline, part of the host model, untouched |
| `Ghost` | added by the cascade, not yet in the host model |
| `Tombstone` | deleted virtually, invisible to matching |

A fourth value exists as a transient. `TentativeTombstone` is what
retraction produces, and it lives only between a retraction and the
following consolidation. It stays matchable on purpose, so a new derivation
can reclaim the identity.

The overlay is insert only. Deletion is tombstoning, and status is filtered
on read.

**Retraction** is what happens when a reason disappears. If a matched
element is gone, every applied rule application whose match contained it has
lost its justification. `Engine::retract_for` follows the provenance edge
from the match to its cascade entry, marks the elements that entry created
as tentative tombstones, and walks on recursively through entries anchored
on those elements. It also forgets the application record, so an identical
re-derivation can apply again and reclaim.

`Engine::consolidate` then decides, for each tentative tombstone only, and
therefore in the size of the delta rather than the model. If a new
derivation reclaimed the identity in the meantime, the element goes back to
alive. If not, it becomes a final `Tombstone`.

**Materialization** is the end of the line. `Graph::materialize` returns a
new graph without tombstones, with every remaining ghost turned into
`Solid`. A connection survives only if both of its endpoints survive. Values
are not copied, because values never lived in the overlay to begin with.
Derived leaves keep their source and their chain, and resolve through the
same resolver as before.

## Values live outside

The graph stores no values. A leaf either has a value in the host, or it is
derived from another leaf through a chain, or it carries a reference into
the rule set's constant table.

The host side is a `ValueResolver`, a single method that maps a baseline
identity to a string. `ValueStore` is the standalone implementation for
tests and benchmarks. `Graph::resolve_value` walks the derivation chain down
to a baseline leaf or a constant, then applies the collected transformations
forward. A chain that does not apply yields no value rather than a wrong
one.

The backward direction works by installing the inverse chain. Lowering
builds it element wise in reverse, so `prefix("get")` becomes a strip and
`capitalize` becomes `decapitalize`. Value resolution then runs that chain
forward like any other. A strip that finds no matching affix yields no
value, which is how an inapplicable backward direction is caught.

A stronger check exists but is not on this path. `Chain::invert_checked`
computes a source from a target value and returns it only if the forward
chain maps that source back onto the target. Its criterion is consistency
with the target value rather than equality with an original the backward
direction cannot know. Nothing inside the library calls it. A caller who
needs the case detection for `capitalize` and `decapitalize` has to call it
directly, because the running path applies those two without verifying.

## What this layer does not do

- There are no negative application conditions. The format has no field for
  them, and a file carrying one is rejected while parsing.
- There is no converter from earlier rule formats, and no way to read them.
- There is no JSON schema for editor support.
- `LoadError` and `PredicateError` implement `Debug`, not `Display` and not
  `std::error::Error`. Callers format them with `{:?}`.
- Lowering errors are a single string type, `LowerError`, without structured
  fields. Most cases that used to end there are now load errors with a
  location.
- At most one connection exists per source and target pair. The graph is a
  set, not a multigraph.
- `capitalize` and `decapitalize` are not invertible on every value, which
  is why the backward direction verifies instead of trusting.
- The regex subset check does not recognize nested character class set
  operations such as `[a-z&&[^aeiou]]`. The permitted syntax subset does not
  contain them anyway.
- `rules::export` is a verification artifact for cross language equivalence,
  not a transport path. Rules travel as declarative files, and each language
  lowers them itself.
