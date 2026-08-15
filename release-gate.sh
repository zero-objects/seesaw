#!/usr/bin/env bash
# Local release gate for seesaw-tgg.
#
# Mirrors what GitHub Actions runs on push, plus `cargo publish --dry-run`.
# No exit-masking pipes anywhere — each step's exit code propagates and
# the first failure aborts the gate (set -e). Use BEFORE pushing a
# release commit; the dry-run uses --allow-dirty so it also works
# pre-commit (the packaging and verify-build run either way).
#
# Usage:
#   ./release-gate.sh

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

step() {
    printf '\n▶ %s\n' "$1"
}

step "cargo fmt --check"
cargo fmt --all -- --check

step "cargo clippy --all-targets -D warnings"
cargo clippy --all-targets -- \
    -D warnings -D clippy::uninlined_format_args

step "cargo test"
cargo test

step "cargo doc --no-deps (RUSTDOCFLAGS=-D warnings)"
# CI runs cargo doc with `-D warnings`, which catches broken intra-doc
# links and ambiguous references (e.g. a name that is both a fn and a
# module). The local gate must catch these too — otherwise the Docs
# build job fails on push after the gate said „green" (rc4 lesson).
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

step "cargo publish --dry-run --allow-dirty"
cargo publish --dry-run --allow-dirty

echo
echo "✓ Local release gate green — safe to push the release commit."
