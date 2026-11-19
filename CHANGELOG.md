# Changelog

All notable changes to `seesaw-tgg` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 release candidates are tagged `1.0.0-rcN`.

## [Unreleased]

## [2.1.0] — 2026-11-19

The engine finds the matches of 2.0.2 with 2 to 1 000 times fewer
instructions in the seven cells of the performance suite that were
also measured with 2.0.2 (table below).
Graph, cascade and its order, the results of `step`,
`saw_contradiction` and the verdicts of `run` are those of 2.0.2.
Identities are unchanged from 2.0.1. A differential test runs random
models and waves through this engine and the published 2.0.2 and
compares all of these after every wave. The performance suite checks
graph and cascade of every cell with a fingerprint. The fingerprints
are the same with and without the features below and on a big-endian
target (s390x under qemu, six cells).

### Added

- `engine::AnchorIndex` (`build`, `built_for`, `fits`,
  `forget_shared`) and `Engine::with_anchor_index`. The delta-local
  expansion searches a rule only at the pattern positions whose paths
  the neighbourhood of a new node realises (a5a264b). `Engine::new`
  takes the index of its rule set from a store of the process that
  keeps the indices of the last 32 rule sets (4885d8a). A host that
  holds the index itself builds it once and hands it to every engine
  over the same rules.
- Capacity hints: `Graph::reserve`, `Engine::reserve` with
  `engine::Sizes`, `Engine::sizes` and `Engine::reserve_for_recorded`
  (e448221, 1cb8253). The engine reserves nothing on its own. Graph,
  cascade and verdicts are the same with and without a hint.
- `Graph::with_value` and `Graph::with_node_value` hand the value of a
  node to a closure, borrowed where the resolver lends it.
  `ValueResolver::baseline_str` lends a baseline value. It is a
  provided method that lends nothing by default, `ValueStore`
  implements it. `Graph::connection_id` and `Graph::type_count`.
- Feature `low_memory` (53a81d0, df488a1, a feature since 1cb8253):
  the graph keeps nodes and connections in dense vectors behind a map
  of numbers, and the engine keeps its lists of indices per element in
  16 bytes instead of 32.
  17 to 24 % less peak memory in four of five large cells (1 % in the
  fifth), up to 4 % more instructions (6 to 8 % where a run deletes),
  7 to 9 % more time on the two recorded host runs. Graph, cascade and
  verdicts are the same with and without it.
- Feature `parallel_seed` (4182330, a feature since 1cb8253):
  `Engine::seed_with_threads` runs the searches of a seed on several
  threads and records the matches in the order of `Engine::seed`. Not
  on wasm32. A second engine seeding a
  translated model of 126 359 nodes took 301 ms on one thread, 115 ms
  on four and 100 ms on eight (wall time, machine under load).
- The performance suite `benches/suite` (9588008): one cell per
  process, user CPU, peak memory, steps, a fingerprint over graph and
  cascade, on Linux the instructions retired in user mode and hardware
  events. `cargo bench --bench suite -- list` names the cells. The
  workload `pde` replays recorded protocols of a host, which are not in
  the repository (`SEESAW_PDE_DUMP`).
- `difftest/` (7f09f8d), a package of its own and not published: the
  differential test against the published 2.0.2
  (`cd difftest && cargo test --release`).
- `--cfg seesaw_checks` compiles the engine's checks of the expansion
  and the seed against the unfiltered search into the library, so that
  the integration tests run under them.

### Changed

- `seed` searches through the anchor index where the search per rule
  would look at more than four times the nodes of the graph (0dcae91).
  It records the same matches in the same order.
- Matcher, engine bookkeeping and graph do less per step. A candidate
  brings the connection it was reached by, a match is filed under its
  nodes only (87b270a), the ref sequence of a match is held once, a
  pulled match is looked up once. The measurements per change are in
  `benches/suite/results/*/README.md`.
- `GhostId` hashes one word instead of its 32 bytes, and its order is
  written out over four words. A host that iterates a
  `HashMap<GhostId, _>` sees another order. The order in which
  `Graph::nodes` walks the graph changes with it. It is documented as
  unspecified. A capacity hint can change it as well.
- The `Debug` output of `Graph`, `TypeTable`, `KonstTable` and
  `ChainTable` has other fields, among them a number per graph and per
  table that depends on what the process created before.
- `engine::MatchRec` is deprecated. No function hands one out or takes
  one.
- `Engine::rules` documents that the rules of an engine are not to be
  exchanged while it holds matches.
- `perf_trace` (feature, off by default) holds the counters of the
  current engine: 15 fields of `Trace`, `Trace::fields` and `add`
  next to `reset` and `snapshot`. With `parallel_seed` the counters of
  the searches stand in the threads that run them and are missing from
  the snapshot of the calling thread. The fields and functions of the
  first generation had no call site left and are gone.
  `cargo semver-checks` reports this as a major change: fields of
  `Trace` removed and added, the functions `step`, `collect`,
  `applied`, `select_examined`, `walk_visited`, `produce`,
  `is_duplicate`, `update_anchored` and `update_full` removed. The
  module is a diagnostic of the implementation and carries no
  stability promise, so the version is 2.1.0. With the default
  features of the crate and with `low_memory` and `parallel_seed` it
  reports no change that needs a new major version.
- The crate leaves out `benches/suite/results/`. The measurements stay
  in the repository.

### Fixed

- `engine::matcher`: a link without direction that only one of the
  two connections carries returned its match twice and returns it once
  (891a63b). The engine discarded the repetition before. A host that
  calls the search itself and counts matches sees the difference.
- The tests that compare with the Java implementation (cascade,
  identities, predicate table) looked for their files under the path
  of the development tree and skipped themselves in this repository.
  They read `java/src/test/resources/fixtures` now (4c3ea24).

### Performance

Rust, instructions retired in user mode in the measured section, in
millions, binaries with one codegen unit, engine of b9b3ce9 (the
commits after it change paths that no cell runs). Raw data and the
other cells in `benches/suite/results/2026-11-15-auswahl/README.md`.

| cell | 2.0.2 | 2.1.0 | 2.1.0, `low_memory` |
|---|---|---|---|
| `langpack/add/k700/n2000` | 158 773 | 155 | 159 |
| `langpack/backward/k700/n2000` | 81 825 | 138 | 139 |
| `langpack/seed/k700/n2000` | 108 919 | 158 | 161 |
| `f2p/batch/f20000` | 8 842 | 2 661 | 2 723 |
| `f2p/add/f20000` | 14 512 | 3 009 | 3 081 |
| `f2p/delta-add/f20000/d1000` | 593 | 189 | 191 |
| `f2p/delta-delete/f20000/d1000` | 20.5 | 9.4 | 10.0 |

Peak memory at the end of the measured section of `f2p/add/f20000`
(580 006 nodes): 1 372 MiB with 2.0.2, 636 MiB, 483 MiB with
`low_memory`. `langpack` is a synthetic rule set in the shape of a
language pack (1 401 rules over one structural type per side). With
the Java pack of zerodds-pde (1 395 rules) on 40 files of
commonmark-java the cascade steps of the forward wave took 3.9 s
instead of 131.5 s, those of the backward wave 2.7 s instead of
60.3 s, with identical graph and output.

Java, thread CPU of the calling thread in ms, minimum over three
rounds, measured 2026-11-16. L0 is Java 2.0.2 with the anchor index
(fa12974). "Java:Rust" divides by the Rust value of the same cell.
Tables of all stages in `benches/suite/results/2026-11-16-java/README.md`.

| cell | L0 | 2.1.0 | Java:Rust |
|---|---|---|---|
| `langpack/add/k700/n2000` | 377.9 | 57.4 | 2.9 |
| `langpack/delta-delete/k700/n20000/d2000` | 1 022.9 | 160.4 | 1.6 |
| `f2p/add/f20000` | 10 025.0 | 2 301.2 | 2.8 |
| `langpack/engines/k700/n200/e34` | 1 437.2 | 230.7 | 3.0 |
| `pde/open/java40` | 46 042.9 | 7 519.4 | 1.9 |

Over the 17 cells that run a cascade the thread CPU fell 2.7 to 6.6
times, Java:Rust from 4.6 to 33.0 before to 1.1 to 8.7 after (the
`reseed` cells by wall time). Java 2.0.2 without the index took
62 893 ms for `langpack/add/k700/n2000`. The fixes in the Java section
were measured afterwards on part of these cells
(`benches/suite/results/2026-11-18-java/README.md`).
`langpack/delta-delete` took 2 to 4 % more thread CPU there. Which
commit causes it was not separated.

Not measured: for an id the graph does not know, `link_removed` and
`linkRemoved` build an index of the links the graph does not hold
(one pass over the match store per graph), and `producers_of` and
`producersOf` build an index of the connections each cascade entry
created. 2.0.2 looked such an id up once. No cell runs these paths.

### Java

The Java implementation in `java/` is not part of the crate. Its JARs
are attached to the GitHub release.

- The JAR has the version 2.1.0 instead of 0.1.0-SNAPSHOT.
- Anchor index in expansion and seed (`engine/AnchorIndex.java`,
  60284bd, 2c3750e). The way of 2.0.2 stays selectable with
  `new Engine(rules, false)` or `-Dseesaw.anchorIndex=false`. With
  `-Dseesaw.checks=true` both ways run and are compared.
- The performance work of the Rust engine (8d45be1 to 49cf169),
  including `Engine.seedWithThreads(Graph, Map, int)`. Steps, applied
  steps, node count and fingerprint of every measured Java cell equal
  those of the Rust cells. `java/bench/jgate.sh` runs the Java tests
  in three settings, these comparisons, a differential test against
  Java 2.0.2 and the full replay of a recorded host protocol.
- Fixed: matches missed behind a value equality (`SAME_VALUE`, from
  `joins` and `same_value_links`). Java 2.0.2 took the equality into
  the search plan, and a node with the same value that is not a
  neighbour was never tried. `Matcher.plan` checks it on the complete
  binding, as Rust does (c233050). Two leaves without a value are no
  longer equal at a value equality, as in Rust. Java 2.0.2 took them
  as equal. How often a match comes no longer depends on the
  connections between the two ends of the equality.
  `Matcher.linksOk` compares the same way (325cbc4). Found
  in the recorded protocols of zerodds-pde, which Java 2.0.2 left at
  line 27 636 (`open2`) and 76 200 (`open5`). All protocols now run
  through with the steps and fingerprints of Rust.
- New public API: `Graph.uid()`, `Graph.partsOfType`,
  `Graph.idsOfType`, `Graph.connectionId`, `Graph.connectionIdTyped`,
  `Graph.retractNode`, `Graph.settleNode`, `Graph.retractConnection`,
  `Graph.settleConnection`, `Graph.resolveNodeValue`,
  `Graph.addCorrTyped`, `Graph.addGhostTyped`,
  `Graph.addDerivedLeafTyped`, `Graph.addKonstLeafTyped`,
  `Graph.childLeafOfTyp`, the class `Parts`, `Id.bytes()`,
  `AnchorIndex.forgetShared()` and `Engine.seedWithThreads`.
- Changed public types: `Graph.parts(Id)` returns a `Parts` as
  `Iterable<Part>`, not a `TreeSet`, and a cast to `TreeSet` or
  `Collection` fails. `Graph.Slot.parts` has the type `Parts`, is
  `null` in the slot of a connection and is not `final`. `Id`:
  `equals`, `compareTo` and `hashCode` do not follow a change to the
  public array `b` any more. `Graph.partsByOtherType` still returns a
  fresh `ArrayList`.
- Contract made explicit: the rules of an engine are not to be
  exchanged while it holds matches, and `Graph.typesByName` is not to
  be changed. Without checks a violation goes unnoticed, with
  `-Dseesaw.checks=true` it throws.
- Behaviour outside the contract that differs from Java 2.0.2:
  `Engine.elementRemoved(Id)` with the id of a connection kills the
  matches resting on it at the next door that takes a graph.
  `Engine.record(int, Id[])` without a graph drops the index of the
  links not held. `new Engine(rules)` takes the index from a store of
  the process that keeps up to 32 indexes (`AnchorIndex.forgetShared()`
  empties it). `Matcher.findMatchesWithFixed` and `Matcher.enumerate`
  return the match of a link without direction that only one
  connection carries once. `Graph.connected` and
  `Graph.connectedAlive` read through the participation list, so a
  host that removes participations by hand gets `false`. The checks
  (`-Dseesaw.checks=true`) throw at new places.
- `Engine.retractMatch` for a dead match whose key another match now
  carries: the other match takes over the provenance edge, and the
  products applied for it fall when it is retracted (a6102c4,
  89ccf01). Java 2.0.2 left them in place unless the dead match had
  produced them as well and fell with it. Rust has no counterpart,
  `retract_match` is private there.
- `elementDeleted`, `elementChanged`, `retractFor` and
  `drainRetraction` with a connection the passed graph does not hold,
  and `linkRemoved` with the id of a connection, behave as in Java
  2.0.2 (391a8f3, 540f03d).

## [2.0.2] — 2026-08-15

The direction of a synchronisation now holds for a whole realisation
wave, and the Lean proofs ship with the crate.

### Direction lives in the delta, for the whole wave

A declarative rule is lowered into a forward and a backward
operationalisation, and the paper states that an incoming delta selects
which of them may run: "a source-domain delta activates only forward
rules, a target-domain delta only backward ones". The engine routed
only the *first* enumeration that way. Once a forward rule had produced
target-side nodes, delta-local expansion offered the backward rules
again, and what stopped them from firing was the correspondence gate —
after the candidates had been enumerated and matched.

An incoming delta is now admitted as a whole:

```rust
engine.admit_delta(&[DeltaDomain::Source]);   // or Target, or both
```

The union of its domains fixes the admissible directions for the
complete wave — retraction, re-derivation and consolidation included.
Products created during that wave cannot change it, and correspondences
created during it activate further rules of the *same* direction, so a
legitimate continuation is never cut off. `admit_initial()` selects the
un-routed batch semantics that a fresh engine starts in.

Only the anchors of *establishing* correspondence links determine a
rule's input domain; referencing links stay context and activate
nothing.

Measured on 50,000 F2P families: a directed wave enumerates 50,000
candidates where the un-routed run enumerated 100,000, the 50,000
backward candidates drop to zero, the 50,000 forward applications are
unchanged, and the run takes 645–664 ms instead of 888–931 ms. The
un-routed path is untouched — dry-cleaner, Hermann/ASTRA and AADL each
reach an identical final state within measurement noise.

### The three doors, everywhere

`retract_for` never distinguished a deletion from a change; the caller
knew, the engine had to guess from the element's status. `element_deleted`
and `element_changed` have been the documented entry points since 2.0.0,
and every internal caller in both languages now uses them. `retract_for`
and `retractFor` remain, deprecated, for external callers.

### Rust and Java compare their cascades, not their results

The parity test compared final graphs. It now compares the complete
cascade sequence — rules, ranks, references, created nodes and edges —
so a divergence in *how* a result is reached fails there rather than
in a later model.

### Proofs in the repository

`proofs/` now ships with the crate: the Lean 4 development of the core
mechanism (identity, consolidation, projection, retraction, rank,
termination, delta, DPO, fold, decidability). `lake build` completes
without `sorry`.

## [2.0.1] — 2026-08-10

Three defects found in a review of the 2.0.0 release, all in the code
that shipped.

### Identity encoding was ambiguous

`konst(parent, "ab", "c", 0)` and `konst(parent, "a", "bc", 0)
produced the same identity. Not a hash collision — the same input
bytes, before hashing began. Three of the six derivations had two
adjacent variable-length fields with nothing marking the boundary.

Every variable-length field now carries its length as a little-endian
`u32`, and a correspondence's ref list carries its count. Four tests
walk the boundary across every split, so a field added without a
prefix fails there rather than in a model.

**Every identity changes.** Anyone persisting `GhostId` values across
the 2.0.0 boundary will not find them again. The collision was not
theoretical: it occurred in our own test corpus, where it made two
distinct correspondences look like one.

### Retraction stopped at a materialization

`retract_match` checked for `Status::Ghost`, so a folded product —
which is `Solid` — was left standing and the same delta produced no
tombstone at all. The criterion is now provenance: whatever appears in
a cascade entry's `created` list falls with it, nodes and edges alike.
What entered through `add_baseline` appears in no such list and is
never touched.

The retraction stays tentative; consolidation at the end of the run
decides. Re-derived within the run means reclaimed, otherwise it
resolves to a tombstone.

This defect was hidden by the one above: the colliding identity meant
no second product was created, so nobody noticed the first was never
retracted.

### A rank ceiling was void whenever the runtime mask was set

`step_with_limit` chose a candidate below its ceiling and then, with
`active` set, replaced that choice by a search over the whole queue.
Mask and ceiling now intersect. That is precisely the combination
backtracking relies on.

### Also

- A correspondence no longer moves when a node it attests is replaced.
  It attests the translation of exactly one node; at a foreign anchor
  neither its statement nor its identity holds.
- The Java match key used `Integer.toHexString`, which drops leading
  zeros: `01 23` and `12 03` produced the same key, so a legitimate
  match could be discarded as a duplicate. Fixed-width now, plus a
  length check on `Id`.

### Verification

The six identity derivations are now written by a Rust test and read
by the Java one. Until now those values sat hard-coded in the Java
test with a note saying they came from Rust, and nothing that proved
it — which is what made this fix expensive.

Structure is unchanged across the fix: 3498 and 8353 live nodes on
astra, 22 and 1538 applications, only the hashes over identities
differ. Rust 342 tests green, Java 111.

## [2.0.0] — 2026-08-09

This release removes the first engine generation and makes the declarative
rule format the interface. It is not an incremental step, so it is worth
saying plainly what changed in thinking, not only in code.

### What the first generation was good for

The delta and Merkle line worked. It showed that a triple graph grammar
can be driven by deltas instead of by full re-derivation, that ghost
overlays make partial state observable, and that rank plus backtracking
turns a search into something you can reason about. It improved on the
classical formulation in measurable ways.

But the underlying idea was wrong. That engine solved problems inside
itself which do not belong there. Negative application conditions,
typed edges carrying meaning, values entering identity, special-case
transformations in the core: each of these is the engine compensating for
something the model does not say.

### What this release does instead

It moves those problems into the model and into the graphs, where they
belong.

- **No negative application conditions.** Every one we examined turned out
  to be a symptom of missing positive structure. The answer is a marker,
  an extra type, or a missing layer in the metamodel, not a condition in
  the rule.
- **No typed edges.** Connections are anonymous. A role that carries
  meaning becomes a node.
- **Identity is structural.** It derives from parent, type, origin and
  transformation chain. No attribute value enters a hash, so renaming
  cannot disturb identity.
- **No special cases in the core.** A mapping the format cannot express is
  written as rules. The comparator table of the PIL2SPELL case study, six
  operators mapped to language constants, is now six rules with an
  equality predicate and a constant, not a transformation baked into the
  engine.

### What this costs, and it is not free

This only works if the model is good enough. Two arbitrary graphs will not
do. The metamodel has to distinguish what needs distinguishing, name what
needs naming, and carry the layers the transformation needs. Where it does
not, the work does not disappear, it moves to the ingest side or to a
well-formedness layer.

That is the honest trade. The engine gets smaller and provable. The
modelling gets stricter. We think that is the right place for the
difficulty, because a model that cannot express its own distinctions is a
problem worth surfacing, not worth hiding in a matcher.

### Removed

- The first generation in full: `engine`, `rule`, `graph`, `fold`, `ops`,
  `xmi`, `viz`. Users who need it can build against `1.0.1`.
- The JNI bridge, the pilot user interface and the wasm binding, all of
  which were built on it. A wasm build can be generated from the current
  generation when it is needed.
- `docs/principles.md`. The conceptual material belongs to the paper.

### Added

- `v3`: the declarative rule format. Named nodes, explicit anchors,
  mandatory correspondence roles, transformation chains built from five
  domain-free primitives, five value predicates with semantics normalised
  across languages, loading with located errors, lowering into two
  directed creation plans.
- `ident`: the two shared types, `GhostId` and `Status`.
- `docs/using.md` and `docs/architecture.md`, both written for this
  version.

### Changed

- Transformations are chains, canonically normalised, and the chain enters
  identity. Identities from the previous generation are not comparable.
- `v2` remains as the engine below the format. It is not the interface.

## [1.0.1] — 2026-07-03

Security patch. No engine behaviour changes — GhostIds, delta sequences and
final graphs are identical to 1.0.0. Only the XMI interop path (`quick-xml`
dependency) is touched.

### Security

- Bump `quick-xml` from `0.36` to `0.41`, which addresses two DoS advisories
  filed against `0.36.2`:
  - **RUSTSEC-2026-0194** — quadratic runtime when checking duplicate
    attribute names in a start tag (CPU-exhaustion DoS on hostile XMI).
  - **RUSTSEC-2026-0195** — unbounded namespace allocation in `NsReader`
    (memory-exhaustion DoS); `0.41` caps declarations at 256 per element.

### Changed

- `XmiError` gains an `XmlIo(std::io::Error)` variant: quick-xml 0.41
  surfaces reader/writer I/O errors as `std::io::Error` instead of folding
  them into `quick_xml::Error`.
- The attribute reader deliberately keeps `Attribute::unescape_value`
  (now deprecated) rather than switching to `normalized_value`, because the
  latter collapses `\t\r\n` to spaces — verbatim attribute payloads must
  round-trip byte-for-byte.

## [1.0.0] — 2026-06-08

First stable release. The public API and the GhostId structural-identity
contract (Def. 5.3) are now under semantic versioning. The engine has been
validated end-to-end against a real quality-assurance project (a CI/CD
pipeline transformation corpus across 17 platforms) in addition to the
paper's worked examples.

This release also lands the cascade performance work: the incremental matcher
is now **linear in both directions** (forward translation and reverse/CST
reconstruction), where it was previously super-linear (≈ quadratic) per
cascade. Proven **bit-identical to rc10** — same GhostIds, same delta
sequence, same final graph — by the 256-case differential property test, a
full/cached differential over five real paper rule sets (with NACs and
attribute conditions), and a verbatim replay of a large real-world workload
(a 1384-line GitHub Actions config: 2712 → 6769 nodes forward, plus its
reverse cascade). No output changes versus rc10; a pure speed release.

### Changed

- **The cached matcher no longer rebuilds its candidate list every step.**
  It keeps an ordered live-candidate set (a `BTreeSet` in the full
  collector's order) and walks it lazily to the first applicable candidate;
  applied/duplicate/NAC-forbidden entries are removed in O(log n) on marking
  instead of being re-scanned. Dead/NAC tracking is a per-cache generation
  counter (O(1) revival) rather than a per-step `DeadSet` rehash. Structural
  NACs that are monotone under add-only are marked once and skipped
  thereafter. NAC checks short-circuit on the first witness instead of
  collecting the whole forbidden set. Together these break the per-step
  O(graph) and O(steps²) terms — the forward cascade goes from ≈ O(nodes²) to
  ≈ O(nodes).
- **`SetAttr` now triggers a full re-enumeration only for kinds a rule
  pattern attribute-constrains.** A `SetAttr` on any other kind cannot change
  the match set, so it is skipped. This removes a reverse-direction
  re-enumeration storm (a backward cascade sets an attribute per step on
  freshly created kinds that no rule constrains), bringing the reverse
  cascade from flat-no-speedup to ≈ 4× and linear.
- **Internal lookup indices (`node_index`, `edge_index`, the id memo) use a
  fast, deterministic, dependency-free FxHash** instead of SipHash. These
  indices take non-adversarial keys; the fixed-seed hasher keeps iteration
  deterministic (≥ the previous bit-identity guarantees). Per-match `bindings`
  use a `BTreeMap` (a handful of small string keys, faster and deterministic
  for that size).

### Added

- `cascade_step_cached` is now public — the single cached step, so callers
  can drive the incremental matcher step by step (e.g. live monitoring),
  mirroring the already-public `cascade_step`.
- `perf_trace` feature (off by default): deterministic structural counters
  for the cascade, to diagnose which phase scales with graph size. No effect
  on the default build's hot path.

## [1.0.0-rc10] — 2026-06-06

Cascade performance overhaul. The matcher is now incremental and the
GhostId hash is faster — proven bit-identical to the previous behavior by a
256-case differential property test (random fwd/bwd/delete/retraction
sequences) plus the full scenario suite.

### Changed

- **GhostId hash: SHA-256 → blake3.** Still cryptographically
  collision-resistant (the structural identity contract of Def. 5.3 holds),
  but SIMD-accelerated and without the software-crypto fallback. A
  content→id memo avoids recomputing the same id within a cascade.
- **`run_cascade` is now the incremental, cache-backed matcher by default.**
  Each step previously re-enumerated all rules over the whole graph
  (O(graph)/step); it now keeps a per-rule match cache and, after an add,
  only extends it with matches that involve the new element
  (O(local)/step). `creator_of` is O(1) (HashMap index) instead of a linear
  scan, removing an O(steps²) cost in contradiction detection.

### Added

- `run_cascade_full` — the full-re-enumeration runner, kept as the
  differential reference.
- `run_cascade_cached`, `collect_candidates_cached`, `MatchCache` — the
  incremental matcher surface.

### Breaking

- **GhostId byte values change** (blake3 ≠ SHA-256). IDs are internally
  consistent within a run, but `idFull`/`seesualId` values persisted from a
  prior SHA-256 release will not match on reload — re-stamp on first open.

## [1.0.0-rc9] — 2026-06-01

Bugfix: a reused correspondence partner is now registered for edge
resolution, so edges created from it are no longer dropped.

### Fixed

- **Reuse-path edge resolvability.** When a creating rule re-fires on an
  anchor that already participates in a correspondence, the recognition
  branch reuses the existing partner instead of minting a new node. It
  propagated the bound attributes but did not register the reused partner
  in the instantiation's `created` map — so the separate `edges_to_create`
  pass could not resolve an endpoint pointing at the reused node, and the
  edge was silently dropped at the resolution guard. Concretely: when
  several elements attach to one shared, reused container, only the first
  (which *creates* the container) kept its membership edge; every
  subsequent element reused the container and lost it. The reuse branch now
  inserts the reused partner into `created`. No new `GhostId` is minted, so
  rename identity stability is unaffected; re-emitted pre-existing edges are
  idempotent (the structural, full-id-keyed `add_edge`/`is_duplicate`). The
  reuse path now differs from the create path only in "no new node," not
  additionally in "endpoint not resolvable." Covered by
  `reuse_path_emits_membership_edge_to_shared_collection`.

## [1.0.0-rc8] — 2026-05-31

Round-trip hardening: correspondence-driven deletion, recognition of the
creating rule across directions, and delta-routed bidirectional cascades.

### Added

- **Correspondence-following retraction.** `retraction_cascade_for` now
  walks `corrL`/`corrR` from a deleted node to its correspondence node and
  on to the partner on the opposite domain, tombstoning the whole triple.
  A delete on one side propagates to the other without a dedicated delete
  rule; the walk is orientation-agnostic, so it carries both forward and
  backward deletes.
- **`directional_rule_refs(&rules, &delta_kinds)`** in the `engine` API:
  selects, per delta, only the rules whose input domain matches the kinds
  that changed. This is how a single session runs both directions without
  ping-pong — a UML-side change activates the forward rules, a Java-side
  change the backward rules.
- **Created-node invariant, enforced at compile time.** Every node a
  rule's creation block produces must carry a correspondence (its
  `GhostId` is rooted there and retraction reaches it via `corrL`/`corrR`).
  `compile` now rejects a creation block with an uncorresponded node
  (`CompileError::CreatedNodeWithoutCorrespondence`) instead of producing a
  silently unmaterializable, undeletable node.

### Changed

- **Recognition of the *creating* rule across directions.** A rule that
  establishes a correspondence, run in the opposite direction over an
  already-translated pair, now reuses the existing correspondence (matched
  by anchor + partner identity) and propagates only the bound attributes,
  instead of minting a duplicate "ghost twin" that tripped the
  name-uniqueness gate. Forward-then-backward over translated structure is
  now a true no-op.
- **`input_domain_kinds` precision.** A rule's activation kinds are now
  derived from the node kinds of its established-correspondence anchors,
  rather than an L-minus-R set difference — so a `{JavaField}` delta
  activates only the attribute rule's backward direction, not the
  getter/setter rules.

### Internal

- The `seesaw-jni` session wrapper (not part of this crate) gained
  baseline-graph mirroring for `DelNode`/retraction tombstones, so a delete
  applied as a baseline mutation survives the next `consolidate` instead of
  being resurrected. The crate-level behaviour is the correspondence-driven
  deletion above; this note records the integration-side counterpart.

## [1.0.0-rc7] — 2026-05-30

Directional rule lowering and identity decoupling — the foundation for
clean rename and round-trip behaviour.

### Added

- **A7 — directional rule lowering.** `compile_bidirectional` emits two
  `CompiledRuleSpec`s per declarative rule (forward `→`, backward `←`),
  with the context-vs-creation role and span anchor derived per direction
  from the correspondence link's role and bindings. `directed_spec` swaps
  L↔R and the correspondence endpoints.
- **`CorrRole`** (`Establishes` / `References`) on `CorrespondenceLinkSpec`
  — makes the correspondence's role explicit in the rule format
  (optional, defaults to none for backward compatibility). A7 uses it to
  derive context vs. creation per direction.

### Changed

- **A8 — identity decoupling.** Correspondence- and R-side `GhostId`s are
  derived from structure plus identity-bearing creation attributes only;
  bound (propagated) values become `SetAttr` ops applied to the target
  after creation. A rename on the L side re-derives the same target (the
  structural op is an idempotent no-op) and updates only the value — no
  duplicate-with-new-identity.

## [1.0.0-rc6] — 2026-05-23

### Fixed

- **Bug #2 (rc5 regression).** Split `attrs_to_set` from `creation_attrs`
  so an attribute set on a context node no longer collides with the
  identity attributes of an R-only created node. Includes a regression net
  for the collision case.

## [1.0.0-rc5] — 2026-05-23

### Fixed

- **B5/rc4 regression.** `is_duplicate` now recognises `SetAttr` ops, so a
  cascade that only updates attributes terminates instead of looping.

## [1.0.0-rc4] — 2026-05-23

### Fixed

- **B5 — rules can set attributes on context nodes.** `compile` previously
  pushed both L- and R-pattern constraints into the match plan
  indiscriminately, so a rule with differing L/R literals on the same
  context node could never match. R-constraints are now classified
  (identical to L → dropped; differing literal → `attrs_to_set`;
  non-literal differing → `CompileError::NonLiteralRAttrUnsupported`), and
  `instantiate` emits the corresponding `SetAttr`.

### Changed

- `release-gate.sh` extended with a `cargo doc -D warnings` check; doc-link
  hotfix for `crate::rule::instantiate`.

## [1.0.0-rc3] — 2026-05-22

### Fixed

- **B4 — rules can create edges between context nodes.**

## [1.0.0-rc2] — 2026-05-20

### Changed

- **B2 — edge-guided cascade matcher.** Match complexity reduced from
  O(Mᴺ) to O(M·dᴺ⁻¹) by guiding candidate enumeration along edges.

## [1.0.0-rc1] — 2026-05-16

- Initial public release of `seesaw-tgg`: a Triple Graph Grammar engine
  with strictly monotonic change handling and rank-based backtracking.

[2.1.0]: https://github.com/zero-objects/seesaw/compare/v2.0.2...v2.1.0
[1.0.0-rc8]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc7...v1.0.0-rc8
[1.0.0-rc7]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc6...v1.0.0-rc7
[1.0.0-rc6]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc5...v1.0.0-rc6
[1.0.0-rc5]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc4...v1.0.0-rc5
[1.0.0-rc4]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc3...v1.0.0-rc4
[1.0.0-rc3]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc2...v1.0.0-rc3
[1.0.0-rc2]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc1...v1.0.0-rc2
[1.0.0-rc1]: https://github.com/zero-objects/seesaw/releases/tag/v1.0.0-rc1
