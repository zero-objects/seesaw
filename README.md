# seesaw-tgg

[![crates.io](https://img.shields.io/crates/v/seesaw-tgg.svg)](https://crates.io/crates/seesaw-tgg)
[![docs.rs](https://docs.rs/seesaw-tgg/badge.svg)](https://docs.rs/seesaw-tgg)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

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
seesaw-tgg = "2.0"
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

## Status

`2.0.0` removed the first engine generation. It is not an incremental
step; see [CHANGELOG.md](CHANGELOG.md) for what changed in thinking,
not only in code. Users who need the previous generation can build
against `1.0.1`.

`2.0.1` fixes three defects found in a review of that release, one of
them in the identity encoding. **Every identity changes between 2.0.0
and 2.0.1**, so persisted `GhostId` values do not carry across. Use
`2.0.1`.

The engine is covered by unit, integration and property tests,
including reproductions of published research cases (FASE 2019, JOT
2022, LMCS 2024, STTT 2021, TTC 2015).

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
