# Changelog

All notable changes to `seesaw-tgg` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 release candidates are tagged `1.0.0-rcN`.

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
