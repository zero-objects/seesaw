# Architecture

How `seesaw-tgg` is wired internally. Module by module, with the data
flow that ties them together.

For the *why*, see [principles.md](./principles.md). For practical usage
with code, see [using.md](./using.md).

## 1. Module map

```
seesaw_tgg
│
├── graph    typed attributed graphs, Solid/Ghost/Tombstone status,
│            parent-rooted GhostId
│
├── ops      atomic operations (Op enum) — AddNode, AddEdge,
│            DelNode, DelEdge, SetAttr
│
├── rule     rule specification format (JSON), compilation,
│            instantiation; RuleSpec → CompiledRuleSpec → Rule
│
├── engine   pattern matching, Rule trait, rank-based selection,
│            cascade step + backtracking
│
├── fold     consolidation (Ghost → Solid), diff against a previous
│            baseline
│
├── xmi      reader for EMF-style XMI 2.0 source models
│
└── viz      DOT output for visualization / debugging
```

Dependencies flow downward: `engine` and `rule` build on `graph` + `ops`;
`fold` builds on `graph`; `xmi` and `viz` are auxiliaries.

## 2. Lifecycle

A session in `seesaw-tgg` is just a `TypedGraph` plus a `Cascade` plus
some rules. There is no `Session` struct; the lifecycle is direct.

```
   ┌────────────────────────────────────────────────────────────┐
   │  TypedGraph::new()                                         │
   │  Cascade::new()                                            │
   │  let rules = [demo_rule_instantiated("R_Class"), …];       │
   └────────────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌────────────────────────────────────────────────────────────┐
   │  build / seed the source side                              │
   │    g.add_baseline_node("Model", "m1", attrs)               │
   │    g.add_edge(…)                                           │
   └────────────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌────────────────────────────────────────────────────────────┐
   │  drive the cascade                                         │
   │    loop { cascade_step(&mut cascade, &mut g, &rules) }     │
   │      ↳ select highest-rank applicable rule                 │
   │      ↳ fire it: writes Ghost nodes / edges into g          │
   │      ↳ record in cascade for backtracking                  │
   │      ↳ stop when Converged or DeltaTouch or RolledBack     │
   └────────────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌────────────────────────────────────────────────────────────┐
   │  observe                                                   │
   │    g.iter_nodes() / g.iter_edges()                         │
   │    each NodeData has type_id, attrs, status                │
   └────────────────────────────────────────────────────────────┘
                              │
                              ▼
   ┌────────────────────────────────────────────────────────────┐
   │  fold (when ready to consolidate the baseline)             │
   │    consolidate(&mut g)                                     │
   │      ↳ Ghost → Solid, Tombstone removed                    │
   │    cascade cleared for the next round                      │
   └────────────────────────────────────────────────────────────┘
```

Three things in this flow are worth pausing on:

- **No half-states.** The cascade either runs to a clean termination or
  backtracks to where it started. The graph between `cascade_step`
  calls is always coherent.
- **Backtracking is local to one cascade.** It does not roll back the
  baseline; only the `Ghost` overlay.
- **Folding is explicit.** The caller decides when to consolidate. This
  is what lets one cascade observe the result of a prior one without
  committing it.

## 3. Graph internals (`graph`)

`TypedGraph` is the working surface for everything else.

- `NodeData { id: GhostId, type_id: String, attrs: BTreeMap<String, String>,
  status: Status }`
- Edges carry type, attrs, and status the same way.
- Iteration: `iter_nodes()`, `iter_edges()`, `get_node(&id)`.

### `GhostId`

A 32-byte SHA-256 hash, displayable in two forms:

- `id.short()` — first 4 bytes as 8 hex chars. Use for logs.
- `id.hex()` — full 64 hex chars. Use when round-tripping through an
  external boundary (e.g., a JNI host) that needs to refer back to
  this exact node. `GhostId::from_hex(s)` parses it back.

Two ways to construct one:

- `GhostId::from_opaque(s)` — for *external identifiers* that you carry
  in from outside (file paths, EMF URIs, JDT handles). The engine hashes
  the string. Same opaque → same id.
- `GhostId::from_parent(parent, edge_kind, type_id, &attrs)` — for
  *cascade-derived* nodes. The id is parent-rooted, structural, and
  includes the identity-bearing attrs but **not** propagated values.
  See [principles.md §7](./principles.md#7-identity-from-structure).

### `Status`

```
Solid               → committed; part of the baseline
Ghost               → live in the current cascade
TentativeTombstone  → marked for deletion; eligible for resurrection
                      if the same id is re-derived this cascade
Tombstone           → definitive; will be removed at the next fold
```

The `TentativeTombstone` step is what makes seesaw idempotent under
re-derivation. See [principles.md §4](./principles.md#4-delta-semantics-and-ghost-projection).

## 4. Operations (`ops`)

Five atomic ops carry every change:

```rust
pub enum Op {
    AddNode { parent: GhostId, edge_type: String,
              type_id: String, attrs: BTreeMap<String, String> },
    AddEdge { source: GhostId, target: GhostId,
              type_id: String, attrs: BTreeMap<String, String> },
    DelNode { target: GhostId },
    DelEdge { target: GhostId },
    SetAttr { target: GhostId, key: String, value: String },
}
```

Each op has a `target` (the element it touches) for *rollup overlay*
semantics: two ops with the same target in the same delta are merged
under "the later one wins".

`AddNode` is the only op that creates a `GhostId`. The id is parent-rooted
and includes the op's `attrs` and `edge_type` — so the *same* `AddNode`
called twice produces the same id. This is the basis for the idempotent
cascade.

## 5. Rules (`rule`)

A rule has two layers:

- **`RuleSpec`** is the declarative format. JSON-compatible. Carries
  `l_pattern`, `r_pattern`, `correspondence_links` (with optional
  `role: Establishes | References`), `nacs`, `rank`, plus the
  `creation_attrs` block that marks which attributes are
  identity-bearing on R-only creation nodes.

- **`CompiledRuleSpec`** is the lowered form the engine actually
  matches. The lowering picks the direction (Fwd or Bwd), swaps
  L↔R + corr endpoints for backward direction, and derives the
  context-vs-creation role of each correspondence from `role` + the
  bindings.

> **Invariant — every created node carries a correspondence.** A node
> that a rule *creates* (an R-only node in `nodes_to_create`) is
> materialized by `instantiate` only as the target of an *Establishes*
> correspondence — its `GhostId` is rooted at the correspondence node, and
> deletion reaches it by following `corrL`/`corrR`. A created node with no
> correspondence is therefore not a "lightweight" node — it is silently
> unmaterializable and unreachable by retraction. `compile` rejects such a
> rule (`CompileError::CreatedNodeWithoutCorrespondence`) instead of
> dropping the node at production time. If several target nodes belong to
> one source element (e.g. a `JavaField` *and* a `Getter` for one
> `Attribute`), give each its own correspondence to that same source
> element (fan-out) — see the demo `R_Getter`/`R_Setter` rules. Pure
> target-side "skeleton" structure is modeled the same way: every node
> corresponds to the source element whose projection it is part of.

### Bidirectional lowering

```
RuleSpec ──compile_bidirectional──► [CompiledRuleSpec ("R→"),
                                     CompiledRuleSpec ("R←")]
                                          │
                                          ▼
                                    instantiate(&compiled) → Box<dyn Rule>
```

`compile_bidirectional` always emits two directed rules per declarative
rule, named `"<name>→"` and `"<name>←"` (U+2192 / U+2190). You register
**both**; the direction is chosen **per delta**, not by a manual pass
switch. Each compiled rule carries `input_domain_kinds` (the L- resp.
R-domain kinds it consumes). After applying a Δ, derive the set of kinds
it touched and call `directional_rule_refs(&rules, &delta_kinds)` to get
the rules whose input kinds intersect the Δ (undirected rules — empty
`input_domain_kinds` — are always included); pass that slice to
`run_cascade`. This is what keeps the bidirectional set from
ping-ponging: a Δ on the L-domain activates only `R→`, a Δ on the
R-domain only `R←`. Among the active rules, rank decides which fires when.

```rust
let spec: RuleSpec = …;                            // one direction-neutral rule
let rules: Vec<Box<dyn Rule>> = compile_bidirectional(&spec)?
    .iter().map(instantiate).collect();            // register both R→ / R←
// per delta:
let active = directional_rule_refs(&rules, &delta_kinds);
run_cascade(&mut cascade, &mut graph, &active, max_steps)?;
```

The `Rule` trait — `pub trait Rule: Debug + Send + Sync` — is what the
engine consumes. The default implementation is `BasicRule`, but any
type can implement it.

## 6. Matching (`engine`)

A pattern is a set of `NodePattern`s + `EdgePattern`s with variables.
Matching produces a `PatternMatch` — a map from variable to `GhostId` in
the live graph.

### Pattern kinds

- **`NodePattern`** specifies a `kind` (the `type_id`) and zero or more
  `AttrPredicate` constraints (literal, regex, exists).
- **`EdgePattern`** specifies source variable, target variable, and a
  `type_id`. It also carries a **`membership`** flag (default `false`):
  when set, the matcher ignores edge direction *and* edge type — the
  two endpoints only need to share any edge, in either orientation.
  This is what makes correspondence matching orientation-agnostic
  (see [principles.md §8](./principles.md#8-symmetric-correspondence)).

### Finding matches

- `find_matches(pattern, graph)` returns every `PatternMatch` of
  `pattern` in `graph`.
- `find_matches_with_fixed(pattern, fixed_bindings, graph)` lets you
  pin some variables — useful when you have an anchor and want
  completions.

## 7. Cascade and backtracking (`engine`)

```
Cascade { entries: Vec<DeltaEntry> }
```

A `Cascade` is the audit trail of an in-flight cascade. Each
`DeltaEntry` records its `origin` (the seed `User` delta, or a `Rule`
application), the `rank` at which it fired, the `op_star` it produced,
the `anchor` nodes it referenced, and — for rule applications — the
match `bindings` (pattern variable → `GhostId`). The first entry is
always the user delta; the rest are the rule applications it induced.

### `cascade_step`

```rust
pub fn cascade_step(
    cascade: &mut Cascade,
    graph: &mut TypedGraph,
    rules: &[&dyn Rule],
) -> Result<TerminationState, EngineError>
```

One step does the following:

1. Collect candidates: for each rule, ask "does your L-pattern match
   the current graph?" via `find_matches`. Filter out matches that
   violate any NAC of the rule (`nacs_forbid`).
2. Rank-order the candidates: `select_highest_rank` picks the
   highest-ranking applicable rule + match.
3. Run the rule: compute its ops, check duplicates (`is_duplicate`)
   and contradictions (`is_contradictory_with_cascade`).
4. Apply the non-duplicate ops to the graph as `Ghost`.
5. Record the application in `cascade`.

`TerminationState` is one of `Running`, `Converged`, `DeltaTouch`,
`RolledBack`.

### Running the cascade

For most uses, call `run_cascade` instead of `cascade_step` in a loop.
For observation between steps (logging, tracing), use
`run_cascade_observable` with a callback. For derivations that may
need to be undone wholesale, `run_cascade_with_rollback` snapshots the
graph before starting.

### Backtracking

When a step would *touch the original delta* — write into a node or
edge that was already modified by the delta that started the cascade —
the engine retreats. The exact mechanism:

1. The latest applied rule (highest rank applied) is removed.
2. Its ops are reversed: ghosts deleted, edges retracted via
   `retraction_cascade_for`.
3. The cascade re-runs from the previous state with that rule excluded
   from the candidate pool.

If the backtracking exhausts the rule space, the outer call returns
`TerminationState::RolledBack`. The graph is restored to its pre-cascade
state; the caller decides what to do.

### Correspondence-following retraction

`retraction_cascade_for(op, graph)` is the same primitive backtracking
uses in step 2, but it is also the engine's **delete-propagation**
mechanism in its own right. Given a `DelNode { target }`, it returns the
follow-up ops that complete the deletion:

1. Tombstone every edge incident to `target`.
2. For each incident `corrL`/`corrR` edge, walk to the correspondence
   node, then across its *other* correspondence edge to the partner on
   the opposite domain. Tombstone both the partner and the corr node.

So deleting one side of a translated pair tombstones the whole triple:
delete a `JavaClass` and its `corr` leads the cascade to tombstone the
corresponding UML `Class` (and vice versa). This is what makes a delete
on **R** propagate to **L** without a dedicated "delete rule" — the
correspondence graph carries deletion in both directions, exactly as it
carries context (see
[principles.md §8](./principles.md#8-symmetric-correspondence)).

The edge-walk is orientation-agnostic: it does not care whether the
deleted node sits on the `corrL` or `corrR` side, so the same code
propagates forward deletes and backward deletes.

> **Integration note.** When a host applies a delete as a *baseline*
> mutation (outside an active cascade), the induced tombstones must reach
> the baseline graph too, not just the ghost overlay — otherwise the next
> `consolidate` resurrects the deleted node from the unchanged baseline.
> The JNI session mirrors retraction tombstones into the baseline while
> the cascade is empty, the same way it mirrors baseline `SetAttr`/
> `AddNode`.

## 8. Fold (`fold`)

A fold consolidates the current `Ghost` overlay into a new baseline.

- `consolidate(base, cascade) -> Result<Consolidated, …>` — folds the
  cascade's `Ghost` overlay onto the `base` baseline into a fresh
  `new_baseline`: every `Ghost` becomes `Solid`; every `Tombstone` (and
  resolved `TentativeTombstone`) is removed. It does not mutate `base` —
  the caller swaps in `new_baseline` and bumps `baseline_counter`.
- `diff(prev_baseline, current_baseline)` — produces the net ops
  between two baselines, suitable for replication, journaling, or
  patching another graph.

Folding is a checkpoint operation. Between folds, the engine remains
fully observable; after a fold, the consolidated state becomes the new
ground truth and the cascade history is cleared.

## 9. External identity

The engine sees nodes by `GhostId`. The outside world sees nodes by
file paths, EMF URIs, JDT handles, or whatever an integration carries.

The bridge is `GhostId::from_opaque(s)`: pass an external identifier
in, get a deterministic GhostId out. The mapping is one-way and
content-based — same string in, same id every time, with no
state needed on either side.

When an integration **needs to register an external identifier for a
node the cascade has already created** — common case: a forward
materializer writes a file, then needs the *file's* identifier to map
back to the cascade-derived JavaClass — there are two patterns:

1. **Direct on `TypedGraph`.** The integration adds an `AddNode` whose
   `parent`, `edge_type`, `type_id`, `attrs` reproduce the rule's
   identity recipe. The resulting `GhostId` matches what the cascade
   derived. (The `id.hex()` / `from_hex` round-trip is the
   loss-free transport for the recipe's output.)

2. **Wrapper-managed map.** The integration maintains its own
   `external_opaque → GhostId` table outside the engine. A backward
   delta resolves external opaques to ids via this table before
   handing them to the cascade.

The `seesaw-jni` host crate implements pattern 2 as a `Session` wrapper
around `TypedGraph`. Most Rust-only users do (1).

## 10. Snapshot

`seesaw-tgg` does not have a built-in JSON snapshot endpoint — the host
integration owns serialization format. The pattern is:

```rust
let nodes: Vec<_> = g.iter_nodes()
    .map(|n| serde_json::json!({
        "id":      n.id.short(),
        "idFull":  n.id.hex(),
        "type":    n.type_id,
        "status":  format!("{:?}", n.status),
        "attrs":   n.attrs,
    }))
    .collect();
```

Two fields are worth including in any host serializer:

- `id.short()` — 8 hex chars, human-readable in logs.
- `id.hex()` — 64 hex chars, the full id. Round-trips losslessly via
  `GhostId::from_hex(s)`.

Edges follow the same shape with `source.short()` / `target.short()`.

---

## Where to go next

- [principles.md](./principles.md) — the conceptual layer.
- [using.md](./using.md) — code patterns, worked examples, pitfalls.
- The paper, for the formal claims that back the guarantees in §6 and §7.
