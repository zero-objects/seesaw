# Release Process

`seesaw-tgg` is published to [crates.io](https://crates.io/crates/seesaw-tgg)
**manually from the maintainer's local environment**, not from CI. The token
stays in `~/.cargo/credentials.toml` (set once via `cargo login`) and never
touches GitHub secrets.

## Cutting a release

1. Bump `version` in `Cargo.toml`.
2. Update `README.md` if APIs changed.
3. Run the full local gate:
   ```sh
   ./release-gate.sh
   ```
   This runs `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
   `cargo test`, and `cargo publish --dry-run --allow-dirty`. Each step's
   exit code propagates — the first failure aborts the gate. **Don't pipe
   any of these commands through `tail`/`grep`/etc. on the command line —
   the pipe's last process is what bash sees as the exit code, so a
   failing `cargo fmt --check | tail -3` looks green.** That's why this
   gate is a script.
4. Commit + push to `main`. Wait for GitHub Actions to confirm the same
   gates pass on the `stable` and `1.88` Rust matrix.
5. Tag:
   ```sh
   git tag -a vX.Y.Z -m "seesaw-tgg X.Y.Z"
   git push origin vX.Y.Z
   ```
6. Publish:
   ```sh
   cargo publish
   ```

The crate appears on crates.io within ~30 s and on docs.rs within a few
minutes after the build queue processes it.

## Yanking

If a release has a critical issue, yank it (it stays in the index for
existing users but is no longer selected for new dependents):

```sh
cargo yank --version X.Y.Z
```

## Verification after publish

```sh
curl -sSL "https://crates.io/api/v1/crates/seesaw-tgg" \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['crate']['max_version'])"
```
