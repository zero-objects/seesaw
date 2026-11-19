# seesaw-tgg

[![crates.io](https://img.shields.io/crates/v/seesaw-tgg.svg)](https://crates.io/crates/seesaw-tgg)
[![docs.rs](https://docs.rs/seesaw-tgg/badge.svg)](https://docs.rs/seesaw-tgg)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.21954055.svg)](https://doi.org/10.5281/zenodo.21954055)

**Bidirectional graph transformation with a declarative rule format.**

A Triple Graph Grammar relates two models by rules that read in both
directions: the same rule that builds the right side from the left
builds the left from the right. This crate is one engine for that, plus
the format its rules are written in.

Three properties shape everything else.

**Identity is structural.** A node's identity derives from its parent,
its type, its origin and the chain of transformations that produced it.
No attribute value enters the hash. Renaming something therefore cannot
disturb its identity, and applying a rule twice cannot produce two
nodes where there should be one.

**Connections are anonymous.** An edge has a direction and nothing
else. A relationship that carries meaning becomes a node, which means
it can be matched, corresponded and deleted like anything else.

**Rules are purely positive.** There are no negative application
conditions. Every one we examined turned out to be a symptom of missing
positive structure, and the answer was a marker, an extra type or a
missing layer in the model, not a condition in the rule.

## Quick start

```toml
[dependencies]
seesaw-tgg = "2.1"
```

A rule set is JSON. This one relates a UML class to a Java class and
carries the name across:

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
          { "name": "cls",   "type": "Class" },
          { "name": "cname", "type": "name" }
        ],
        "links": [["model", "cls"], ["cls", "cname"]]
      },
      "right": {
        "anchor": "jcls",
        "nodes": [
          { "name": "jmodel", "type": "Model", "same_as": "model" },
          { "name": "jcls",   "type": "JavaClass" },
          { "name": "jname",  "type": "name" }
        ],
        "links": [["jmodel", "jcls"], ["jcls", "jname"]]
      },
      "corrs": [
        {
          "type": "CorrClass", "left": "cls", "right": "jcls",
          "role": "establishes",
          "bindings": [{ "left": "cname", "right": "jname" }]
        }
      ]
    }
  ]
}
```

Loading it gives two creation plans per rule, forward and backward:

```rust
use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::Status;
use seesaw_tgg::plan::DirectedRule;

fn run(rule_file: &str) {
    let mut g = Graph::default();
    let rules = seesaw_tgg::rules::load(rule_file, &mut g).expect("rule file loads");

    // A source model: Model → Class → name leaf.
    let model = g.add_baseline("m", "Model");
    let cls = g.add_baseline("m/Person", "Class");
    let cname = g.add_baseline("m/Person/name", "name");
    g.connect(model, cls, Status::Solid);
    g.connect(cls, cname, Status::Solid);

    // Values live in the host, not in the graph.
    let mut values = ValueStore::default();
    values.insert(cname, "Person");

    let rules: &'static [DirectedRule] = Box::leak(rules.into_boxed_slice());
    Engine::new(rules).run(&mut g, &values, 1000);
    // The graph now holds a JavaClass, a CorrClass, and a name leaf
    // that resolves to "Person" without ever storing it twice.
}
```

## Documentation

Two pages, written for this version:

- **[docs/using.md](docs/using.md)** — how to write a rule set, load
  it, drive a cascade and read the result. Field by field through the
  file format, a worked example, the error messages you will meet.
- **[docs/architecture.md](docs/architecture.md)** — how the pieces
  fit. From rule file through validation and lowering to the engine,
  how identity is derived, what the lifecycle states mean.

## Paper and proofs

The engine implements a published design. The paper states the
formal core, the termination and consolidation results, and the
measurements:

> Sandra Theresa Keßler and Thomas Maier. *seesaw: Delta-extended
> Triple Graph Grammars with Constructive Termination, Irredundant
> Consolidation, and Non-destructive History.*
> [10.5281/zenodo.21954055](https://doi.org/10.5281/zenodo.21954055)

**[proofs/](proofs/)** holds the Lean 4 development: 23 verification
obligations, machine-checked, no `sorry`. Seventeen use no
project-specific assumption; six rest on one of three named interface
idealisations, each isolated in its own axiom cluster. It builds
against Lean-Core alone, without Mathlib, in under a second:

```sh
cd proofs && lake build
```

`proofs/README.md` maps each obligation to its theorem and file.

## Modules

| module | what lives there |
|---|---|
| `rules` | the rule format: reading, validation, lowering |
| `graph` | the model: nodes, connections, types, values |
| `plan` | what a lowered rule is, and how it is applied |
| `engine` | the delta-local cascade with retraction |
| `ident` | `GhostId` and `Status`, shared by all of the above |

`rules::load` is the one way from a rule file to plans. The layers
below become visible only where a cascade is driven or a graph is read.

## Many engines, large models

The delta-local expansion searches only where a match can start. It
finds those places through an anchor index built from the rules.
`Engine::new` takes the index from a store of the process that keeps
the indices of the last 32 rule sets, so a host that makes one engine
per run builds it once. A host that holds the index itself builds it
with `AnchorIndex::build` and hands it to `Engine::with_anchor_index`.
`AnchorIndex::fits` tells whether an index belongs to a rule set, and
`AnchorIndex::forget_shared` empties the store of the process.

A host that knows how large a run gets can make room before it starts:
`Graph::reserve`, `Engine::reserve` with an `engine::Sizes`, and
`Engine::sizes` for what a run reached. `Engine::reserve_for_recorded`
makes room for what the recorded matches would create. The engine
reserves nothing on its own. Graph, cascade and verdicts are the same
with and without a hint. Only the order of `Graph::nodes`, which is
unspecified, can differ.

```rust
use seesaw_tgg::engine::{AnchorIndex, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::plan::DirectedRule;

fn two_engines(rules: &[DirectedRule], g: &mut Graph, values: &ValueStore) {
    let index = AnchorIndex::build(rules);

    let mut first = Engine::with_anchor_index(rules, &index);
    first.run(g, values, 1_000_000);

    // A later engine over the same rules reads the same index and
    // starts with the room the first one needed.
    let mut second = Engine::with_anchor_index(rules, &index);
    second.reserve(first.sizes());
    second.run(g, values, 1_000_000);
}
```

`Graph::with_value` and `Graph::with_node_value` hand the value of a
node to a closure. A rule constant, and a baseline leaf whose resolver
lends its value through `ValueResolver::baseline_str`, arrive borrowed,
without a new `String`. `ValueStore` lends its values. The trait method
has a default that lends nothing, so an existing resolver keeps
compiling. `Graph::connection_id` reads the id of a connection from the
graph, and `Graph::type_count` counts the nodes of a type.

## Features

All three are off by default.

```toml
[dependencies]
seesaw-tgg = { version = "2.1", features = ["low_memory"] }
```

- `low_memory`: less memory for more work. The graph keeps nodes and
  connections in dense vectors behind a map of numbers, and the engine
  keeps its lists of indices per element in 16 bytes instead of 32.
  Measured on the performance suite: 17 to 24 % less peak memory in
  four of five large cells (1 % in the fifth), up to 4 % more
  instructions (6 to 8 % where a run deletes). Time on a machine under
  load: 7 to 9 % more on the two recorded host runs, no difference
  beyond the noise on the generated workloads. Graph, cascade and
  verdicts are the same with and without it.
- `parallel_seed`: `Engine::seed_with_threads` runs the searches of a
  seed on several threads and records the matches in the order of
  `Engine::seed`. What the engine does afterwards is the same. The
  resolver is asked from several threads at once and has to be `Sync`.
  Not on wasm32.
- `perf_trace`: structural counters of the cascade in the module
  `perf_trace` (searches, candidates, node checks, value resolutions,
  identity hashes and others). They do not depend on the machine. The
  counters serve diagnostics and follow the internals of the engine.
  They carry no stability promise: a counter can change or disappear
  in any release, a minor one included.

```rust
use seesaw_tgg::engine::Engine;
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::plan::DirectedRule;

#[cfg(feature = "parallel_seed")]
fn seed_on_four_threads(engine: &mut Engine<'_>, g: &Graph, values: &ValueStore) {
    engine.seed_with_threads(g, values, 4);
}

#[cfg(feature = "perf_trace")]
fn print_counters(rules: &[DirectedRule], g: &mut Graph, values: &ValueStore) {
    seesaw_tgg::perf_trace::reset();
    Engine::new(rules).run(g, values, 1_000_000);
    for (name, count) in seesaw_tgg::perf_trace::snapshot().fields() {
        println!("{name}\t{count}");
    }
}
```

## Status

`2.0.0` removed the first engine generation. It is not an incremental
step; see [CHANGELOG.md](CHANGELOG.md) for what changed in thinking,
not only in code. Users who need the previous generation can build
against `1.0.1`.

`2.0.1` fixes three defects found in a review of that release, one of
them in the identity encoding. **Every identity changes between 2.0.0
and 2.0.1**, so persisted `GhostId` values do not carry across.

`2.0.2` derives the direction of a cascade from the incoming delta
instead of a manual switch, so a source-side change activates only
forward rules and a target-side change only backward ones. Identities
are unchanged from `2.0.1`.

`2.1.0` finds the matches of `2.0.2` with less work and adds the API
and the features above. Identities, graph, cascade and the verdicts of
`run` are those of `2.0.2`. A differential test runs random models and
waves through this engine and the published `2.0.2` and compares both
after every wave. Details and measurements are in
[CHANGELOG.md](CHANGELOG.md). Use `2.1.0`.

The engine is covered by unit, integration and property tests,
including reproductions of published research cases (FASE 2019, JOT
2022, LMCS 2024, STTT 2021, TTC 2015).

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
