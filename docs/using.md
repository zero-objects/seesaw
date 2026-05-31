# Using `seesaw-tgg`

Practical guide. The conceptual layer lives in
[principles.md](./principles.md); the internal mechanics in
[architecture.md](./architecture.md). This page shows how to actually
use the crate, with runnable examples and a list of the mistakes that
are easy to make in the first hour.

## 1. Install + quick-start

```toml
[dependencies]
seesaw-tgg = "1.0.0-rc8"
```

The minimum session is `TypedGraph` + `Cascade` + some rules + a step
loop:

```rust
use std::collections::BTreeMap;
use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::demo::demo_rule_instantiated;

let mut g = TypedGraph::new();
let root = g.add_baseline_node("Unknown", "root", BTreeMap::new());
let model = g.add_baseline_node(
    "Model", "mDemo",
    [("name".into(), "Demo".into())].into_iter().collect(),
);
g.add_edge(root, model, "contains", BTreeMap::new(), Status::Solid).unwrap();

let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
let rules: Vec<&dyn Rule> = vec![r_class.as_ref()];

let mut cascade = Cascade::new();
let _ = cascade_step(&mut cascade, &mut g, &rules).expect("step");
```

There is **no `Session` struct in this crate**. The session is the
graph and the cascade you hold on to. Wrap them in your own type if you
want a tighter interface; the `seesaw-jni` crate shows one such wrapper.

## 2. Define a rule set in JSON

The native rule format is JSON. The same JSON is exchanged with host
adapters, so the schema is the source of truth.

```json
{
  "name": "my-grammar",
  "rules": [
    {
      "name": "R_Class",
      "rank": 40,
      "documentation": "Class on L corresponds to JavaClass on R.",
      "l_pattern": {
        "nodes": [
          { "id": "m", "kind": "Model",  "constraints": [] },
          { "id": "c", "kind": "Class",  "constraints": [] }
        ],
        "edges": [
          { "kind": "classes",
            "source_node_id": "m", "target_node_id": "c" }
        ]
      },
      "r_pattern": {
        "nodes": [
          { "id": "m",  "kind": "Model",     "constraints": [] },
          { "id": "jc", "kind": "JavaClass", "constraints": [] }
        ],
        "edges": [
          { "kind": "javaClasses",
            "source_node_id": "m", "target_node_id": "jc" }
        ]
      },
      "correspondence_links": [
        {
          "l_node_id": "c",
          "r_node_id": "jc",
          "kind": "CorrClass",
          "role": "Establishes",
          "attribute_bindings": [
            { "l_attr_name": "name", "r_attr_name": "name",
              "transformation": "identity" }
          ]
        }
      ]
    }
  ]
}
```

Key fields:

- `rank` — strict total order across the rule set. Higher = tried first.
- `l_pattern` / `r_pattern` — node lists with `id` (pattern variable),
  `kind` (the graph's `type_id`), and `constraints` (attribute
  predicates).
- `correspondence_links` — connect an L variable and an R variable. The
  `role` is optional and one of `"Establishes"` (this rule creates the
  correspondence) or `"References"` (it uses an existing one as
  context). If omitted (`null` / absent), the default establishes
  behaviour applies.
- `attribute_bindings` — value propagation from L attr to R attr.
  `transformation: "identity"` is the common case.

Load via the spec parser and the bidirectional lowering:

```rust
use seesaw_tgg::rule::{
    compile::compile_bidirectional, instantiate::instantiate,
    spec::parse_ruleset,
};

let rs = parse_ruleset(json_str).expect("valid JSON");
let mut compiled: Vec<_> = Vec::new();
for r in &rs.rules {
    for c in compile_bidirectional(r).expect("compile") {
        compiled.push(c);
    }
}
let rules: Vec<Box<dyn seesaw_tgg::engine::Rule>> =
    compiled.iter().map(instantiate).collect();
```

For ad-hoc demo work the embedded ruleset is available:

```rust
use seesaw_tgg::rule::demo::{demo_rule_instantiated, DEMO_RULE_NAMES};
```

## 3. Worked example A — forward cascade

UML→Java direction. The same shape as the embedded
`examples/basic_cascade.rs`:

```rust
use std::collections::BTreeMap;
use seesaw_tgg::engine::{cascade_step, Cascade, Rule, TerminationState};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::demo::demo_rule_instantiated;

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k,v)| (k.to_string(), v.to_string())).collect()
}

let mut g = TypedGraph::new();
let root  = g.add_baseline_node("Unknown", "root", BTreeMap::new());
let model = g.add_baseline_node("Model", "mDemo", attrs(&[("name","Demo")]));
let class = g.add_baseline_node("Class", "cWidget", attrs(&[("name","Widget")]));
g.add_edge(root,  model, "contains", BTreeMap::new(), Status::Solid).unwrap();
g.add_edge(model, class, "classes",  BTreeMap::new(), Status::Solid).unwrap();

let r_class = demo_rule_instantiated("R_Class").expect("R_Class");
let r_attr  = demo_rule_instantiated("R_Attr").expect("R_Attr");
let rules: Vec<&dyn Rule> =
    vec![r_class.as_ref(), r_attr.as_ref()];

let mut cascade = Cascade::new();
let state = cascade_step(&mut cascade, &mut g, &rules).expect("step");
assert_eq!(state, TerminationState::Running);
```

After the step, the graph contains the original `Class` and `Model`
plus a `Ghost` `JavaClass`, a `Ghost` `CorrClass`, and the
`corrL`/`corrR`/`javaClasses` edges that anchor them.

Run the full version end-to-end: `cargo run --example basic_cascade`.

## 4. Worked example B — backward cascade

The *same* declarative rule serves both directions. The engine picks
the direction by which side already exists.

```rust
use seesaw_tgg::rule::compile::compile_bidirectional;
use seesaw_tgg::rule::demo::demo_ruleset_spec;
use seesaw_tgg::rule::instantiate::instantiate;

// Java side only: Model("Demo") --javaClasses--> JavaClass("Foo")
let mut g = TypedGraph::new();
let root  = g.add_baseline_node("Unknown", "root", BTreeMap::new());
let model = g.add_baseline_node("Model", "mDemo", attrs(&[("name","Demo")]));
let jc    = g.add_baseline_node("JavaClass","jcFoo",attrs(&[("name","Foo")]));
g.add_edge(root,  model, "contains",     BTreeMap::new(), Status::Solid).unwrap();
g.add_edge(model, jc,    "javaClasses",  BTreeMap::new(), Status::Solid).unwrap();

// Lower R_Class to both directions, pick the backward direction.
let spec = demo_ruleset_spec();
let r_class_spec = spec.rules.iter()
    .find(|r| r.name == "R_Class").unwrap();
let directed = compile_bidirectional(r_class_spec).unwrap();
assert_eq!(directed.len(), 2, "Fwd + Bwd");
let r_class_bwd = instantiate(&directed[1]);  // index 1 = backward
let rules: Vec<&dyn Rule> = vec![r_class_bwd.as_ref()];

let mut cascade = Cascade::new();
let state = cascade_step(&mut cascade, &mut g, &rules).expect("step");
assert_eq!(state, TerminationState::Running);
```

After the step, a `Ghost` `Class("Foo")` and a `Ghost` `CorrClass`
appear on the UML side, anchored to the same `Model` node.

Run the full version end-to-end: `cargo run --example backward_cascade`.

### Routing a full bidirectional rule set

The example above hand-picks `directed[1]` to isolate one direction.
A real round-trip loads *both* directions of *every* rule and lets the
delta decide which fire. Lower each rule with `compile_bidirectional`,
instantiate all the directed forms, then — per delta — call
`directional_rule_refs(&rules, &delta_kinds)` to select only the rules
whose input domain matches what changed:

```rust
use seesaw_tgg::engine::directional_rule_refs;

// One-time: lower every rule to all its directions.
let mut rules: Vec<Box<dyn Rule>> = Vec::new();
for r in &spec.rules {
    for directed in compile_bidirectional(r).expect("compile") {
        rules.push(instantiate(&directed));
    }
}

// Per delta: gather the kinds it touched (e.g. {"JavaClass"} for a
// Java-side edit) and route only the matching direction.
let refs = directional_rule_refs(&rules, &delta_kinds);
run_cascade(&mut cascade, &mut g, &refs, max_steps).expect("cascade");
```

This is the routing that makes forward and backward coexist in one
session without ping-pong: a UML-side change activates the forward
rules, a Java-side change the backward rules, and a rule with no
matching input kind simply does not fire this round. Registering the
whole set but *not* routing it — running every direction on every
delta — is the most common cause of a cascade that never converges.

## 5. Worked example C — rename + identity stability

The point of **A8** (identity decoupling): an attribute that the rule
*propagates* is not part of the corresponding R node's id. Renaming on
L re-derives the same R id; the new value flows through as a
`SetAttr`.

```rust
// Seed: Class("Foo") on the L side.
let mut g = TypedGraph::new();
let root  = g.add_baseline_node("Unknown", "root", BTreeMap::new());
let model = g.add_baseline_node("Model","mDemo",attrs(&[("name","Demo")]));
let class = g.add_baseline_node("Class","cFoo", attrs(&[("name","Foo")]));
g.add_edge(root,  model, "contains", BTreeMap::new(), Status::Solid).unwrap();
g.add_edge(model, class, "classes",  BTreeMap::new(), Status::Solid).unwrap();

let r_class = demo_rule_instantiated("R_Class").unwrap();
let rules: Vec<&dyn Rule> = vec![r_class.as_ref()];

// First cascade — derive JavaClass("Foo").
let mut c1 = Cascade::new();
cascade_step(&mut c1, &mut g, &rules).unwrap();
let id_before = g.iter_nodes()
    .find(|n| n.type_id == "JavaClass").unwrap().id;

// Rename on L.
g.set_node_attr(&class, "name", "Bar");

// Second cascade — the JavaClass's id stays put; only name propagates.
let mut c2 = Cascade::new();
cascade_step(&mut c2, &mut g, &rules).unwrap();
let after: Vec<_> = g.iter_nodes()
    .filter(|n| n.type_id == "JavaClass").collect();

assert_eq!(after.len(), 1);
assert_eq!(after[0].id, id_before, "id is structural, not name-based");
assert_eq!(after[0].attrs.get("name").map(|s| s.as_str()), Some("Bar"));
```

The full example also asserts the invariants and prints the matching
ids: `cargo run --example rename_identity`.

## 6. Lifecycle: submit, cascade, snapshot, fold

A typical request/response cycle in a host integration:

```
   apply delta(s) to the graph                 ← caller's job
        │
        ▼
   loop { cascade_step(&mut cascade, &mut g, &rules) }
        │
        ▼
   inspect: g.iter_nodes(), g.iter_edges()
        │
        ▼
   fold::consolidate(&mut g)                   ← when ready
        │
        ▼
   cascade.clear()                             ← start fresh for next round
```

Two practical notes:

- The cascade modifies the graph in place. If you want the option to
  reject the result, call `engine::run_cascade_with_rollback` instead
  of looping `cascade_step` yourself.
- `fold::consolidate` makes ghosts permanent. **Do not fold while a
  cascade is still in flight.** Fold between cycles.

## 7. Reading the graph (and snapshots)

The graph offers direct iteration:

```rust
for n in g.iter_nodes() {
    println!("{} ({:?})  type={}  attrs={:?}",
        n.id.short(), n.status, n.type_id, n.attrs);
}
```

If you serialize for a host, include both forms of the id. The short
form is for humans; the full form round-trips losslessly through
`GhostId::from_hex`:

```rust
serde_json::json!({
    "id":     n.id.short(),       // 8 hex chars, log-friendly
    "idFull": n.id.hex(),         // 64 hex chars, round-tripable
    "type":   n.type_id,
    "status": format!("{:?}", n.status),
    "attrs":  n.attrs,
})
```

Status names worth knowing in serialized output:

| Status | When you'll see it |
|---|---|
| `Solid` | Part of the baseline. Was there at the start of the cascade. |
| `Ghost` | Created or modified by the current cascade. |
| `TentativeTombstone` | About to be deleted, but eligible for resurrection if re-derived this cascade. |
| `Tombstone` | Definitively gone at the next fold. |

## 8. External integration

The engine identifies nodes by `GhostId`. The outside world identifies
them by paths, URIs, handles. Bridge with `GhostId::from_opaque`:

```rust
use seesaw_tgg::graph::GhostId;
let id = GhostId::from_opaque("platform:/resource/foo.model#//@root");
// Same string in → same id every time. No state.
```

Two patterns for keeping the engine and an external world in sync:

1. **Recipe-replay.** When the cascade has created a node and an
   external boundary needs to recreate the same id later, replay the
   recipe: build the same parent, the same edge kind, the same
   identity-bearing attrs. `GhostId::from_parent` produces a stable id
   from those inputs. The full `id.hex()` is a loss-free transport.

2. **External map.** Maintain a `(external_id → GhostId)` map outside
   the engine. Resolve external ids before handing them to the engine.
   `seesaw-jni` follows this pattern.

Pick (1) when the recipe is short and the boundary can compute it
deterministically. Pick (2) when the boundary already has its own
identity scheme and you just need to bridge.

## 9. Pitfalls

The list of things that look reasonable but bite.

- **Marking-NACs.** Don't use NACs to mark "I already fired here, don't
  fire again." The engine's idempotence comes from stable ids and the
  `TentativeTombstone` resurrection step, not from absence-of-marker
  conditions. A marking NAC will block legitimate re-derivation.
  *Symptom:* a rule that should re-fire after a tombstone/resurrection
  sequence simply does not run.

- **Putting propagated values in the id.** Don't include attributes
  bound from L into the R node's identity recipe. They are *values*;
  putting them in the id means a rename produces a new node and a
  duplicate. See worked example C.

- **Folding mid-cascade.** `consolidate` between `cascade_step` calls
  collapses the in-flight ghosts into baseline before backtracking can
  retract them. Always loop the cascade to a terminal state first.

- **Concurrent sessions on a shared graph.** A `TypedGraph` is owned;
  the cascade mutates it. Run sessions in separate graphs and fold
  diffs together if you need parallelism.

- **Confusing `Ghost` with "wrong".** `Ghost` means *derived in this
  cascade*. Not committed yet, not invalid. Treat ghost nodes as full
  members of the graph until you've decided to fold or roll back.

- **Skipping the `consolidate` call.** Ghosts accumulate. If you never
  fold, every cascade replays every prior derivation.

- **A created node with no correspondence.** Every node a rule's
  creation block produces must be tied to a correspondence — its
  `GhostId` is rooted there, and deletion only reaches it by following
  `corrL`/`corrR`. A "lightweight" created node with no corr is silently
  unmaterializable and invisible to retraction. `compile` rejects such a
  rule up front (`CreatedNodeWithoutCorrespondence`). If one rule must
  spawn several R-side nodes (a scalar plus its sequence wrapper, say),
  give *each* its own correspondence to the same source anchor rather
  than hanging extras off one corr. *Symptom:* a compile error naming
  the offending variable — or, if you bypass compile, a node that never
  appears in the output and never gets deleted. See
  [architecture.md §5](./architecture.md#5-rules-rule).

## 10. Diagnostics

Useful signals during development:

- `cascade.len()` — how many rule applications the engine recorded in
  the current pass.
- `g.node_count()` / `g.edge_count()` — sanity numbers to log between
  cascades.
- `TerminationState::DeltaTouch` — a rule wanted to touch the original
  delta. Investigate the rule; if it's correct, declare it lower-rank.
- `EngineError` cases from `cascade_step` — surface them up rather than
  swallowing. They distinguish "applicable but blocked by NAC" from
  "applicable but contradictory" from "no candidate".

The `viz` module emits a DOT graph for any state. Render with
`graphviz` to see what the cascade has built:

```rust
let dot = seesaw_tgg::viz::dot::to_dot(&g);
std::fs::write("session.dot", dot).unwrap();
// dot -Tsvg session.dot -o session.svg
```

## 11. Testing your rules

Three layers of test that cover most regressions:

1. **Unit-level on the spec.** Parse the JSON. Run `compile` and
   `compile_bidirectional`. Check the resulting `CompiledRuleSpec`
   names (`<name>→` / `<name>←`) and that no rules failed to compile.

2. **Cascade fixtures.** Build a small seed graph, register the rules,
   step the cascade, assert against the resulting node + edge set. The
   embedded `examples/` directory has the shape: seed → cascade → sort
   the nodes → compare.

3. **Identity invariants.** For any rule that propagates an attribute,
   write a *rename* test: derive the R node, modify the L attribute,
   re-cascade, assert the R id stayed put. This is the A8 contract;
   regressions show up as duplicates.

For property-based tests, the `proptest` dev-dependency is already
configured; the existing `tests/proptest_invariants.rs` is a starting
point for what kinds of invariants are worth fuzzing.

---

## Where to go next

- [principles.md](./principles.md) — why the engine is shaped this way.
- [architecture.md](./architecture.md) — what each module does.
- `cargo doc --open` for the Rustdoc reference.
