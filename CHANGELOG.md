# Changelog

All notable changes to `seesaw-tgg` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 release candidates are tagged `1.0.0-rcN`.

## [Unreleased]

### Delta-local expansion anchors only where a match can start

After every applied step, `expand_at` anchored each created node at
every active rule at every pattern position of the same type and ran
one anchored search per triple. In rule sets whose structural nodes
share one type and whose node correspondences share one corr type —
the shape a language pack produces — nearly every rule has such a
position, and what tells the rules apart (a leaf value two to four
links away) was checked only inside the search. Each step paid one
search per rule.

The engine now keeps an anchor index (`engine/anchor.rs`): per rule and
position the paths of the pattern from that position (breadth-first
spanning tree, up to six links), merged per position type into one
prefix tree. A new node's neighbourhood is walked once along that tree;
a (rule, position) is searched only if all its paths are realised and
its own value predicate holds. The condition is necessary for a match,
checked on the same graph state, and the searches run in the order of
the former loop, so the recorded matches and their order are unchanged. `elements_added` uses the same
index.

`Engine::new` builds the index on first use. A host that runs several
engines over the same rules builds it once with `AnchorIndex::build`
and passes it to the new `Engine::with_anchor_index(rules, &index)`; an
index built for other rules is ignored and the engine builds its own.

Measured with the Java pack of zerodds-pde (1 395 rules, 2 790 directed)
on 40 files of commonmark-java: cascade steps of the forward wave
131.5 s → 3.9 s, of the backward wave 60.3 s → 2.7 s; identical graph
and output. The Java implementation has the index as well (below).

### Further changes to expansion, seed, matcher and graph

Found with the counters and profiles of the performance suite (below).
None of them changes what the engine does: graph, cascade and its
order, the sequence of results `step` returns, `saw_contradiction` and
the verdicts of `run` are those of 2.0.2. The suite checks graph and
cascade with a fingerprint per cell, a differential test runs random
models and waves through this engine and the published 2.0.2 (below).

Expansion:

- The anchor index compared every place of a node's type against each
  walk. Each place with a path is now filed under one of its path
  nodes, split by the value the place asks of the node itself, and a
  walk looks only at the places under the nodes it marked.
- The walk reads types, directions and values, not the status of the
  elements on a path. The condition stays necessary; a path over a
  retracted element leads to a search that fails on that element.
- A match that binds several of the new nodes was found once from each
  of them and dropped again by the match store. The search anchored at
  a new node now leaves out the nodes that stand earlier in the same
  delta, so the match is found from the first of them, at the point in
  the order where it was recorded first before. The walk of the index
  leaves the same nodes out, so no search is started that could only
  find such a match (in `f2p/add` five of eight searches found nothing).
- The plan of an anchored search depends on rule and position only. It
  is built at the first search at its place and kept.
- An anchored search writes into buffers the engine keeps and no longer
  allocates per search and per match.
- The candidate triples of a large delta are sorted per node and
  distributed by rule instead of sorted as a whole.
- The index is built without copying the paths of every position (it
  was more than half of a run over a small model with 1 401 rules).
- A walk looks a neighbour up once, for its value and its
  participations.

Seed:

- `seed` searched every rule over all nodes of the type of its first
  position. In a rule set whose rules share one structural type that is
  rules times nodes. Where the search per rule would start at more than
  four times the nodes of the graph, the seed now anchors every node
  through the index, like an added one, and records the matches rule by
  rule in the order of the search per rule. The state of the engine
  after the seed is the same.

Matcher:

- A candidate taken from a participation list comes with the connection
  of the link it was reached by. The matcher reads that connection's
  status instead of calling `Graph::connected`, which derives the
  connection id with a blake3 hash. The check over all links at the end
  of a search is reduced to the links no step has checked (value joins,
  self links, links between two pre-bound positions). A direction-free
  link that only one of the two connections carries used to return the
  same match twice and now returns it once.
- Candidates are tried as they are read, without collecting them first.
- Value predicates read the value of a rule constant or of a baseline
  leaf borrowed, without building a `String`
  (`ValueResolver::baseline_str`, `Graph::with_value`).

Engine bookkeeping:

- The ref sequence of a match is held once and shared by match store,
  todo, key index and applied set (four copies before).
- The match store reads the ids of a match's connections from the graph
  instead of deriving each with a hash.
- The identities of the nodes a step creates are derived once, for the
  duplicate check, the contradiction check and the creation.
- The per-element index lists hold up to four entries in place.
- The todo key carries the head of the ref sequence, so that most
  comparisons are decided without following the sequence.
- The applied set is a hash set, and a cascade entry takes the created
  nodes without a copy.
- The index from a product to the entries that produced it holds the
  created nodes. The producers of a created connection are found from
  its two ends when a deleted connection asks for them.
- Retraction walks the products of an entry without copying the lists
  and reads and writes each status in one access.

Graph:

- A slot of the map holds a node or a connection, not room for both.
- The participation list of a node is a sorted vector while short and a
  B-tree above 48 entries. It starts with room for two.
- `connect_reporting` reaches each endpoint once (five accesses to the
  map per new connection before).
- The order of `GhostId` is written out over four words instead of a
  call to `memcmp`. Its hash is one word instead of the 32 bytes; the
  order in which `Graph::nodes` walks the map changes with it (it is
  documented as unspecified).
- The range of one counterpart type in a participation list is found by
  comparing types only.
- A new slot is written once.

Tried and taken back, each on a measurement or a test:

- Leaving out a match whose anchor is already translated when it is
  found on the nodes a step has just created, and the search for it
  (two of three steps in `f2p/add` pull such a match and count a
  duplicate). 2.0.2 keeps such a match in its store. A later `seed`
  finds it again; where the translating correspondence had been
  retracted in between, the engine then applied the rule or reported a
  contradiction, and 2.0.2 does neither. `step` also returned fewer
  results, which hosts count. Found by reading and by the differential
  test.
- Equality of `GhostId` written out like its order (slower).
- Reading the connection id from the participation list in
  `Graph::connected` (more instructions in `seed`).
- Collecting the input of an identity hash and hashing it in one call
  (more instructions).
- Folding the hash word of `GhostId` so that identities built by hand
  spread better (1.4 to 3 % more instructions; documented at
  `GhostId::from_raw` instead).

Measured on one machine shared with other projects. The time of a cell
varied by 10 to 15 % between runs of one binary there, so the comparison
is the number of instructions retired in user mode in the measured
section, which is the same in every run of one binary. "index" is the
anchor index alone, "now" all of the above. In millions of instructions:

| cell | 2.0.2 | index | now |
|---|---|---|---|
| `langpack/add/k700/n2000` | 169 583 | 1 343 | 177 |
| `langpack/backward/k700/n2000` | 87 230 | 1 197 | 151 |
| `langpack/seed/k700/n2000` | 115 350 | 9 139 | 180 |
| `langpack/add/k700/n20000` | not run | 12 176 | 1 286 |
| `langpack/delta-add/k700/n20000/d200` | not run | 125 | 23.4 |
| `langpack/delta-change/k700/n20000/d200` | not run | 2 070 | 287 |
| `langpack/delta-delete/k700/n20000/d200` | not run | 162 | 59.1 |
| `f2p/batch/f20000` | 9 002 | 8 516 | 3 187 |
| `f2p/add/f20000` | 14 855 | 13 404 | 3 515 |
| `f2p/delta-add/f20000/d1000` | 610 | 565 | 197 |

Time against 2.0.2, both states measured in alternation (user CPU of
the measured section, minimum per state): `langpack/add/k700/n2000`
20 899 ms → 37.9 ms, `langpack/backward/k700/n2000` 9 503 ms → 26.4 ms,
`langpack/seed/k700/n2000` 12 146 ms → 31.7 ms, `f2p/add/f20000`
3 014 ms → 1 135 ms, `f2p/batch/f20000` 2 110 ms → 1 085 ms. In `f2p`
the gain in time is smaller than the gain in instructions: the cell
retires 0.68 instructions per cycle (2.0.2: 1.17), it waits for memory.
The forward runs of the 34 dry-cleaner fixtures (17 rule sets, 4 006
rules in sum, engine and rules new per run) took 131 ms instead of
377 ms (minimum of five).

Peak memory at the end of the measured section of `f2p/add/f20000`
(580 006 nodes): 745 MiB, with 2.0.2 and the same suite 1 372 MiB.

Graph and cascade of every cell are those of 2.0.2 where 2.0.2 was run
(seven of the cells of these sizes and thirteen cells of the sizes
`k50/n2000` and `f2000`, among them the cells that change and delete),
and equal between all states otherwise. The same fingerprints come out
on a big-endian target (s390x under qemu).

`langpack` is a synthetic rule set in the shape of a language pack
(1 401 rules over one structural type per side), not the Java pack
above. Raw data: `benches/suite/results/`.

### Options measured in combination, and the ones taken

Eleven further changes were each built behind a switch of its own and
kept until all were there. Then every setting was measured: none, all,
each alone, all but each one. The effect of an option alone and its
effect among all others differ by at most three percentage points of
instructions in every cell but one: filing under nodes only saves
2.6 % alone and 11.2 % among the others in
`langpack/engines/k700/n200/e34`. Taken into the engine:

- A match is filed under its nodes only. 2.0.2 filed it also under the
  connection of every link of its pattern; the matches that rest on a
  removed connection are now found from the lists of its two ends. For
  an id the graph does not know (a link the pattern reads against the
  direction the graph holds, or a graph the host has replaced by a
  materialized copy) the engine keeps an index of the links whose
  connection the graph does not hold. It is built at the first such
  question and costs nothing before. From then on `record` files the
  unheld links of every new match in it, until the engine is handed
  another graph. The engine asks such a question itself when a
  retraction on a materialized copy looks for products the copy left
  out.
- `step` looks a pulled match up once. The candidate in the todo
  carries the number of its match, "already applied" is a mark on that
  number, and one access per anchor answers both "tentatively
  retracted" and "already translated".
- The type, constant and chain ids a rule's plan names are resolved in
  the tables of a graph once and kept per rule.
- An engine that was handed no anchor index takes the one of its rule
  set from a store of the process (the last 32 rule sets, compared by
  what an index reads from the rules). A host that makes one engine per
  run builds the index once. `AnchorIndex::forget_shared()` empties the
  store.
- The walk over the anchor index passes once over the participation
  list of a node where the prefix-tree node has four or more groups
  and the list has at most eight entries per group, instead of asking
  the list for one range per group. This is the shape of rule sets
  with many node types.
- Capacity hints: `Graph::reserve`, `Engine::reserve(Sizes)`,
  `Engine::sizes()`, `Engine::reserve_for_recorded`. The engine
  reserves nothing on its own.

Two features, off by default:

- `low_memory`: the graph keeps nodes and connections in dense vectors
  behind a map of numbers, and the engine's lists of indices per
  element take 16 bytes instead of 32. Less memory for more
  instructions (numbers below).
- `parallel_seed`: `Engine::seed_with_threads` runs the searches of a
  seed on several threads and records the matches in the order of
  `seed`. Not on wasm32. A second engine seeding a translated model of
  126 359 nodes took 301 ms on one thread, 115 ms on four and 100 ms on
  eight (wall time, minimum of three samples on a machine under
  load); with 80 001 families 438, 262 and 252 ms. A first seed over a
  model that is not translated finds little, its work is in the steps.

Built, measured and not taken (they stay on the branch `perf/options`):

- The todo as a binary heap. Between 2.2 % fewer and 1.9 % more
  instructions; in `pde/open/open5`, where 9 197 of 1 347 266 steps
  apply a rule, 5.1 % more alone and 8.2 % more among the others.
- `Equals` predicates compared by numbers of values instead of text.
  Among the others at most 1.5 % fewer instructions, 5 to 7 % more
  peak memory in the three `pde` cells below 250 MB, and a new method
  on `ValueResolver`.

The rules of an engine are not to be exchanged while it holds matches:
what a match rests on is read from the pattern of its rule, and its
place in the todo from the rule's rank. `Engine::rules` says so.

Instructions in millions, binaries built with one codegen unit (the
table above was measured with sixteen; with a switch more or less the
split of the crate into units moved, which changed cells by up to a
few percent without a change to the code that runs). "before" is the
state of the sections above:

| cell | 2.0.2 | before | now | now, `low_memory` |
|---|---|---|---|---|
| `langpack/add/k700/n2000` | 158 773 | 178 | 155 | 159 |
| `langpack/backward/k700/n2000` | 81 825 | 150 | 138 | 140 |
| `langpack/seed/k700/n2000` | 108 919 | 181 | 158 | 161 |
| `langpack/add/k700/n20000` | not run | 1 305 | 1 038 | 1 071 |
| `langpack/delta-add/k700/n20000/d200` | not run | 23.5 | 11.4 | 11.6 |
| `langpack/delta-change/k700/n20000/d200` | not run | 289 | 252 | 262 |
| `langpack/delta-delete/k700/n20000/d200` | not run | 59.0 | 52.9 | 56.9 |
| `f2p/batch/f20000` | 8 842 | 3 196 | 2 661 | 2 723 |
| `f2p/add/f20000` | 14 512 | 3 544 | 3 009 | 3 081 |
| `f2p/delta-add/f20000/d1000` | 594 | 199 | 189 | 191 |
| `f2p/delta-delete/f20000/d1000` | 20.5 | 9.7 | 9.4 | 10.0 |
| `pde/open/java40` | not run | 17 475 | 12 895 | 13 178 |
| `pde/open/open5` | not run | 56 752 | 31 809 | 32 934 |
| `langpack/engines/k700/n200/e34` | not run | 2 180 | 473 | 481 |

Peak memory at the end of the measured section (MB, before → now →
`low_memory`): `f2p/add/f20000` 744 → 636 → 484,
`langpack/add/k700/n20000` 215 → 187 → 146, `pde/open/java40`
1 048 → 998 → 832, `pde/open/open5` 2 171 → 1 919 → 1 895.

User CPU, before → now, the two states measured in alternation on a
machine under load (minimum of three runs of two samples each):
`f2p/add/f20000` 1 089 → 998 ms, `f2p/batch/f20000` 1 033 → 765 ms,
`langpack/add/k700/n20000` 330 → 258 ms, `pde/open/java40`
5 168 → 4 401 ms, `pde/open/open5` 7 346 → 4 062 ms. The second sample
of a run takes the anchor index from the store of the process.

`low_memory` against the default, six runs in alternation:
`pde/open/java40` 4 358 → 4 739 ms and `pde/open/open5`
3 971 → 4 262 ms, more in every run (2.5 to 12.7 %). On
`f2p/add/f20000`, `f2p/batch/f20000` and `langpack/add/k700/n20000`
the difference changes its sign from run to run (−9 to +16 %), the
minima lie within 5 % of each other.

The `pde` cells replay recorded runs of zerodds-pde (below). `engines`
runs 34 engines one after the other in one process, the rules loaded
anew for each. Graph and cascade of every cell are the same in all
states, the thirteen small cells and seven of these equal 2.0.2. The
consumers were run against this state: the development copy with the
dry-cleaner fixtures and the dry-cleaner crates pass with the test
counts of 2.0.2, the forward runs of the 34 fixtures take 127 ms
(2.0.2: 377 ms).

### Java: anchor index

The Java engine searches through the anchor index in the expansion and
in the seed (`engine/AnchorIndex.java`). The way of 2.0.2 stays
selectable: `new Engine(rules, false)` or `-Dseesaw.anchorIndex=false`;
with `-Dseesaw.checks=true` both ways run and are compared. Java
reproduces steps, applied steps, node count and fingerprint of the Rust
cells of the language-pack workload at the sizes `k50/n2000`,
`k700/n2000` and `k700/n20000`. `add k700/n2000`: 62 893 ms → 431 ms,
30 769 505 searches → 9 905 (Java 21, one machine under load). Not
ported: leaving the earlier nodes of a delta out of the search.

### Tests

- `difftest/` is a package of its own, not published, that depends on
  this engine and on `seesaw-tgg` 2.0.2 from the registry. Random
  models and sequences of waves (add, change, delete on either side,
  links, deleted products, seeds, `run` with a step budget, waves that
  stop before saturation) go through both; after every wave graph,
  cascade, the results of `step`, `saw_contradiction` and the verdicts
  of `run` are compared. `cd difftest && cargo test --release`. The
  driver gives graph and engine a capacity hint; the features
  `low_memory` and `parallel_seed` are passed on, with the second the
  driver seeds through `seed_with_threads`.
- `--cfg seesaw_checks` compiles the engine's checks of the expansion
  and the seed against the unfiltered search into the library, so that
  the integration tests run under them
  (`RUSTFLAGS="--cfg seesaw_checks" cargo test`). A test build of the
  library has them anyway.
- The comparison files shared with the Java implementation (cascade,
  identities, predicate table) were looked for under the path of the
  development tree. In this repository the tests did not find them and
  skipped themselves. They read `java/src/test/resources/fixtures` now.

### Performance suite

`benches/suite` runs one cell (workload, operation, size) per process
and writes one line per cell: user CPU, wall time, peak memory at the
end of the measured section, steps, a fingerprint over the graph and
the cascade in the order it was applied, and on Linux the instructions
retired in user mode, CPU cycles, cache misses and mispredicted
branches of the measured section. The fingerprint has to be the same
for every build of the engine. `cargo bench --bench suite -- list`
names the cells, `benches/suite/cells.sh` runs a list of them,
`benches/suite/profile.sh` profiles one over the whole process,
`benches/suite/hot.sh` reports the hot paths of the measured section
alone (inclusive and self share per function), and
`benches/suite/lines.sh` resolves the self share of a function into the
functions inlined into it and into source lines. `hot.sh` and
`lines.sh` take the event to sample, `cache-misses:u` among them.

With `--features perf_trace` a cell also reports structural counters
(searches, candidates, node checks, value resolutions, identity hashes
and others). They do not depend on the machine.

The workload `pde` replays a protocol of the calls a host made to
graph, rule loader, anchor index and engines, and checks every call
(ids, the result of each `step`, cascade length, node count); it stops
at the first deviation. The protocols recorded from zerodds-pde are not
in the repository; `SEESAW_PDE_DUMP` names their directory, without it
the cells do not exist. The cells `langpack/engines/…` and
`langpack/index/…` run many engines in one process; with the feature
`parallel_seed` there are cells for the seed on several threads. The
suite gives capacity hints as a host would (`benches/suite/hints.rs`).

`hot.sh` and `lines.sh` build with the features in `SUITE_FEATURES`
and, with `SUITE_CALLGRAPH=fp`, walk frame pointers instead of copying
16 KiB of stack per sample: the copy loses the samples of deep
recursions, a third to a half of the measured section on the `pde`
cells, and not the same share in two states. Two states that differ in
a feature are to be compared with one codegen unit
(`CARGO_PROFILE_BENCH_CODEGEN_UNITS=1`).

### Added

- `engine::AnchorIndex` (`build`, `built_for`, `fits`) and
  `Engine::with_anchor_index`. The engine takes a shared index when the
  patterns of its rules are the ones the index was built from.
- `ValueResolver::baseline_str` (provided, lends nothing by default),
  `Graph::with_value`, `Graph::with_node_value`, `Graph::connection_id`,
  `Graph::type_count`.
- `Graph::reserve`, `Engine::reserve`, `Engine::sizes`, `engine::Sizes`,
  `Engine::reserve_for_recorded`, `AnchorIndex::forget_shared`.
- Features `low_memory` and `parallel_seed` (`Engine::seed_with_threads`).

### Changed

- `engine::MatchRec` is deprecated. No function hands one out or takes
  one; the engine keeps its matches in a record of its own.
- The `Debug` output of `Graph`, `TypeTable`, `KonstTable` and
  `ChainTable` has other fields than in 2.0.2, among them a number per
  graph and per table instance: a counter of the process, so it
  depends on what the process created before.
- `perf_trace` (feature, off by default): the module now holds the
  counters of the current engine. The fields of the first generation
  had no call site left and are gone. The module is a diagnostic of the
  implementation and changes with it.

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

[1.0.0-rc8]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc7...v1.0.0-rc8
[1.0.0-rc7]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc6...v1.0.0-rc7
[1.0.0-rc6]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc5...v1.0.0-rc6
[1.0.0-rc5]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc4...v1.0.0-rc5
[1.0.0-rc4]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc3...v1.0.0-rc4
[1.0.0-rc3]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc2...v1.0.0-rc3
[1.0.0-rc2]: https://github.com/zero-objects/seesaw/compare/v1.0.0-rc1...v1.0.0-rc2
[1.0.0-rc1]: https://github.com/zero-objects/seesaw/releases/tag/v1.0.0-rc1
