# Changelog

All notable changes to `seesaw-tgg` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Pre-1.0 release candidates are tagged `1.0.0-rcN`.

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
