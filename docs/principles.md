# Principles

Why `seesaw-tgg` exists, what it does differently, and the ideas it stands on.

This document is the conceptual layer. Read it once to understand why the
crate is shaped the way it is. The mechanics live in
[architecture.md](./architecture.md); the day-to-day usage in
[using.md](./using.md).

## 1. The classical TGG pathology

Triple Graph Grammars are a formal framework for bidirectional
model-to-model transformations. A grammar describes a source graph **L**,
a target graph **R**, and a correspondence graph **D** that anchors them.
From a single grammar, a TGG tool derives both directions: build **R**
from **L**, build **L** from **R**.

The trouble starts when grammars get generalized. The iterative
rule-matching loop can fall into infinite folding and expansion. A rule
fires, creates structure, that structure causes another rule to fire,
and so on, with no convergence guarantee.

The established mitigations are local patches:

- **Lazy evaluation** delays rule application until forced, deferring
  the symptom but not eliminating it.
- **Negative Application Conditions (NACs)** forbid certain matches,
  preventing some loops but burdening the grammar with bookkeeping that
  has nothing to do with the transformation itself.
- **Application conditions** in general grow the grammar's surface area
  faster than its expressive power.

None of these address the root cause: in a derivation graph that is
allowed to revisit a state with new ghosts, the iteration is not
well-founded.

## 2. The `seesaw-tgg` answer

`seesaw-tgg` replaces the rule-matching loop with a different control
mechanism: **strictly monotonic change handling with rank-based
backtracking**.

Two ideas combined:

- **Strictly monotonic**: every step of the cascade either adds a fresh
  ghost projection or refines one already present. No step ever undoes
  a prior step's structure while remaining in the same baseline. The
  resulting derivation graph is acyclic by construction.

- **Rank-based backtracking**: when a derivation runs into a
  contradiction — most commonly, a propagation that would touch the
  very delta that started the cascade — the engine retreats to the
  highest-rank applied rule and chooses another path. Rank is an
  interface: declare it.

The combination yields termination without NACs and without hand-tuned
application conditions. The grammar describes the transformation; the
engine handles control.

> **Theory.** The monotonicity lemma sits behind every guarantee in this
> document. See the accompanying paper for the proof.

## 3. The L–R–D triple as observable state

The engine holds the three graphs as one observable triple. Edges of
**D** anchor it in both **L** and **R**, and **D** is stable in the
engine's baseline state.

When a delta is applied to **L** — for example, an `AddNode` or a
`SetAttr` — the triple does not move directly. It first **freezes**:
the pre-delta state remains observable, and the post-delta state
becomes the seed of a ghost projection.

This freeze is what makes ranked backtracking possible. The engine can
explore derivations from `L + δ` without ever overwriting `L` itself —
and so without losing the option to retreat.

## 4. Delta semantics and ghost projection

Status is a first-class property of every node and edge:

| Status | Meaning |
|---|---|
| `Solid` | Committed; part of the consolidated baseline. |
| `Ghost` | Derived from the current delta; not yet folded. |
| `TentativeTombstone` | Marked for deletion during a cascade — but eligible for resurrection if another rule re-derives the same ghost id in the same step. |
| `Tombstone` | Definitively gone at the next fold. |

The cascade reads both `Solid` and `Ghost`. It writes `Ghost`. The
`Solid` baseline never moves mid-cascade. Only at fold-time do ghosts
become `Solid` (or vanish if the cascade was rolled back).

The two-stage tombstone — *tentative* then *definitive* — is what makes
seesaw idempotent under re-derivation. If the same delta is replayed,
the cascade re-creates the same nodes with the same ghost ids, finds
the tentative tombstones, and resurrects them. There is no double-add
and no Phantom-Foo.

## 5. Convergence vs. delta-touch

The cascade walks rules in rank order. Each step picks the
highest-rank applicable rule, fires it, and records the application.
The walk ends in one of three states:

1. **Converged.** No more rules apply. Ghosts become candidates for
   the next fold. Clean outcome.

2. **Delta-touch.** A propagation would write into a node or edge that
   the original delta already mutated. In classical TGG this is where
   the infinite-null pathology starts. `seesaw-tgg` treats it as a
   signal to **backtrack** to the highest-rank applied rule and try
   another path.

3. **Capitulation.** Backtracking exhausted the rule space without
   finding a non-touching path. The engine reports the contradiction;
   the caller decides what to do.

Rank gives the engine a deterministic order to explore. It is not a
"best rule" selector.

## 6. Rank as interface, not policy

The engine never mandates what "highest rank" means. It mandates that
the rule set carries a strict total order. Two practical defaults:

- **Declaration order.** The rule listed first has the highest rank.
  Trivial to read, robust across grammar revisions.
- **Domain-specific.** When one rule is strictly more general than
  another, hand-rank the general rule lower so the specific rule fires
  first.

Rank affects *exploration order*. It does not change the set of valid
derivations: two rank orders applied to the same grammar produce the
same converged ghost set, in different orders.

> **Theory.** See the rank-equivalence theorem in the paper.

## 7. Identity from structure

Every node and edge has a `GhostId`. The engine derives it from
**structure** — the parent's id, the rule step that created it, and the
node's *identity-bearing* attributes (the ones marked as such in the
rule's creation block).

Propagated attribute values — the ones the rule binds from **L** to
**R** — are **not** part of the identity. They are written as separate
`SetAttr` operations after the node is created.

The practical consequence: a rename on the **L** side does not change
the identity of the corresponding **R** node. The cascade re-derives
the same `GhostId`; the `SetAttr` for the bound attribute updates the
value in place.

This is the difference between an *identity* and a *property*. The
engine is precise about which is which.

> **Theory.** See Def. 3.2 (parent-rooted ghost id) and the identity
> stability lemma. The mechanism also underpins the resurrection
> behaviour from §4: the same structural cause produces the same id.

## 8. Symmetric correspondence

A correspondence is a triple, not a directed edge. The engine matches
context correspondences orientation-agnostically: a `corr` node must
be connected to both endpoints, regardless of whether the
`corrL`/`corrR` edges happen to be oriented L→D→R or the other way
round.

A correspondence established in one direction — say, forward L→R — is
therefore recognised as context by rules running in the other
direction (backward R→L) without any duplication or re-establishment.

This is what makes round-trips work without bookkeeping. Forward and
backward share the same correspondence graph.

> **Theory.** See the paper section on Way 2 corr-symmetry.

## 9. Baseline and observability

The full state of a session is `(graph, baseline_counter, cascade)`:

- `graph` carries `Solid` + `Ghost` + tombstone nodes/edges.
- `baseline_counter` increments at each fold (B₀, B₁, …). It identifies
  the consolidated state in which the next cascade runs.
- `cascade` records the active derivation: which rules fired in which
  order. This is what backtracking walks.

External integrations can sample the engine at any point — between
deltas, mid-cascade, after a fold. The result is always a coherent
observable view of the triple. The engine never has a half-state.

---

## Where to go next

- [architecture.md](./architecture.md) — module-by-module mechanics:
  how the graph, ops, rules, cascade, and fold are wired.
- [using.md](./using.md) — practical guide with runnable examples: set
  up a session, define rules, drive the cascade, read snapshots.
- The accompanying paper goes deeper on each formal claim.
