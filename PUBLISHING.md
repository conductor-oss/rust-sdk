# Publishing the Rust SDK

Publish `conductor-macros` before `conductor-sdk`. The SDK needs the published macros crate.

## Prepare 0.1.2

1. Review the API changes in [CHANGELOG.md](CHANGELOG.md) and finalize the release notes.
2. Set both crate versions and the root `conductor-macros` dependency to 0.1.2. Update
   `Cargo.lock`.
3. Finalize the changelog and README examples. Use Rust 1.85 or newer.
4. Run the repository checks:

   ```shell
   ./scripts/add-license-headers.sh --check
   cargo fmt --all -- --check
   cargo clippy --lib --all-features -- -D warnings
   cargo clippy --tests --examples --all-features -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
   cargo check --all-features
   cargo test --lib --all-features
   cargo test --doc --all-features
   cargo doc --no-deps --all-features
   ```

## Publish

Requires crates.io ownership of both crates and a Cargo registry token.

```shell
cargo publish -p conductor-macros --dry-run
cargo publish -p conductor-macros
```

Once crates.io indexes `conductor-macros` 0.1.2:

```shell
cargo publish -p conductor-sdk --dry-run
cargo publish -p conductor-sdk
```

Keep the macros path dependency in the root `Cargo.toml`; its version field points to the
published crate for packaging.

Verify both versions on crates.io. Compile the [README quickstart](README.md#agent-quickstart)
in a fresh project, then tag `v0.1.2` and create a GitHub release.
