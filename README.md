# seesaw-tgg

[![crates.io](https://img.shields.io/crates/v/seesaw-tgg.svg)](https://crates.io/crates/seesaw-tgg)
[![docs.rs](https://docs.rs/seesaw-tgg/badge.svg)](https://docs.rs/seesaw-tgg)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**Delta-extended Triple Graph Grammar engine with rank-based backtracking.**

Triple Graph Grammars (TGG) are a formal framework for bidirectional
model-to-model transformations. Their classical formulation has a long-
standing weakness: with generalized rule sets, the iterative rule-matching
loop can fall into infinite folding/expansion. Existing mitigations (lazy
evaluation, application-conditions, NACs) are local patches, not systemic
solutions.

`seesaw-tgg` proposes a different control mechanism: strictly monotonic
change handling with rank-based backtracking.

## The model

A source graph **L**, a target graph **R**, and a correspondence graph **D**
are maintained as an observable triple. **D** is anchored in both **L** and **R**
through its edges and is stable in state `0`.

When a delta is applied to **L** (or **R**), the triple freezes at state `0+1`
(yielding e.g. **L₁**, **D₀**, **R₀**). The cascade then operates on ghost
projections **L'₁**, **D'₀**, **R'₀** — the deltas unfold under rule matching
until one of two cases:

- the model **converges** (clean fold), or
- the propagation directly or indirectly touches the original delta on **L**,
  which in the classical formulation triggers the infinite-null pathology.

At that point, a rank function **rank(r)** strictly monotone over the rule
space **R** governs backtracking: the engine retreats to the highest-rank
applied rule and either (a) takes another path by rank, (b) capitulates, or
(c) takes another path under an alternative resolver. Once convergence is
reached, the ghost derivations of **L₁ R₀ D₀** are re-integrated bottom-up
to a stable **L₂ D₁ R₁**.

Rank is domain-specific. Empirical evidence from adjacent domains suggests
that **declaration order** is the simplest robust rank. The engine treats
rank as an interface, not a fixed policy.

## What's in this crate

- **Graph** — `TypedGraph` with Solid/Ghost/Tombstone status tracking
- **Ops** — `Op::{AddNode, AddEdge, DelNode, DelEdge, SetAttr}` with applicability
  semantics matching the paper definitions
- **Rule** — `RuleSetSpec`, `compile`, `instantiate`. JSON-based rule format.
- **Cascade** — `cascade_step`, `run_cascade` with rank-ordered backtracking
- **Fold** — `consolidate`, `diff` for baseline transitions
- **XMI** — Reader for EMF-style XMI 2.0 models

## Quick start

```toml
[dependencies]
seesaw-tgg = "1.0.0-rc7"
```

```rust
use seesaw_tgg::engine::{cascade_step, Cascade, Rule};
use seesaw_tgg::graph::{Status, TypedGraph};
use seesaw_tgg::rule::demo::demo_rule_instantiated;
use std::collections::BTreeMap;

fn main() {
    // Build a small source graph: Model → Class
    let mut g = TypedGraph::new();
    let model = g.add_baseline_node("Model", "mApp", BTreeMap::new());
    let class = g.add_baseline_node("Class", "cWidget", BTreeMap::new());
    g.add_edge(model, class, "classes", BTreeMap::new(), Status::Solid).unwrap();

    // Load demo rule (UML Class → Java Class bidirectional)
    let r_class = demo_rule_instantiated("R_Class").unwrap();
    let rules: Vec<&dyn Rule> = vec![r_class.as_ref()];

    let mut cascade = Cascade::new();
    let state = cascade_step(&mut cascade, &mut g, &rules).unwrap();

    println!("state: {:?}", state);                  // Running
    println!("cascade length: {}", cascade.len());   // 1
    println!("graph: {} nodes, {} edges",
             g.node_count(), g.edge_count());        // 4 nodes, 3 edges
}
```

See [`examples/basic_cascade.rs`](examples/basic_cascade.rs) for a runnable
demo:

```sh
cargo run --example basic_cascade
```

## Demo rule set

The crate ships an embedded demo rule set (`tests/fixtures/demo-ruleset.json`)
that mirrors the UML-Class → Java-Class transformation used in the paper:

- **R_Class** (rank 40): `Class ↔ JavaClass` under a shared `Model` anchor
- **R_Attr** (rank 30): `Attribute ↔ JavaField` within a corresponding `Class`
- **R_Getter** (rank 20): adds `Getter` method for each `Attribute`
- **R_Setter** (rank 10): adds `Setter` method for each `Attribute`

All four rules are bijective and use the strictly-monotone rank order.

## Status

`1.0.0-rc7` — release candidate. API may still see breaking changes before
`1.0.0`. The core algorithms (cascade, fold, ranked backtracking) are stable
and test-covered across ~265 unit, integration, and property tests including
benchmark suites against published research cases (FASE 2019, JOT 2022,
LMCS 2024, STTT 2021, TTC 2015).

## License

Apache License 2.0 — see [LICENSE](LICENSE) and [NOTICE](NOTICE).
