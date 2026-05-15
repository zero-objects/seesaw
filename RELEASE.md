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
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings -D clippy::uninlined_format_args
   cargo test
   cargo publish --dry-run
   ```
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
