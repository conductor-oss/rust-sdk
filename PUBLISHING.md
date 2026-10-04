# Publishing the Rust SDK

`conductor-macros` and `conductor-sdk` are separate crates. Publish the macros crate first;
`conductor-sdk` 0.1.1 requires `conductor-macros` 0.1.1 from crates.io, even though the
repository uses a path dependency for local development.

## Prepare 0.1.1

1. Confirm the release's public API is compatible with the published 0.1.0 crate. Cargo
   resolves a `0.1` dependency to both 0.1.0 and 0.1.1, so breaking changes in 0.1.1
   affect existing users on `cargo update`. Review the breaking changes listed in
   [CHANGELOG.md](CHANGELOG.md) before publishing.
2. Set the version in both `Cargo.toml` files and the root crate's
   `conductor-macros` dependency to `0.1.1`. Update `Cargo.lock`.
3. Finalize the 0.1.1 changelog entry and README examples. Use Rust 1.85 or newer for
   the SDK's declared minimum version.
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

5. Validate the agent quickstart against a Conductor server with a configured model:

   ```shell
   scripts/validate-local-agent.sh openai/gpt-4o-mini
   ```

   The model argument is explicit. The provider credential belongs in the Conductor
   integration, not in the example or this script.

## Publish

The publisher must own both crates on crates.io and have a configured Cargo registry
token. Publishing is irreversible: an existing crate version cannot be overwritten.

```shell
cargo publish -p conductor-macros --dry-run
cargo publish -p conductor-macros
```

Wait until crates.io indexes `conductor-macros` 0.1.1, then verify and publish the SDK:

```shell
cargo publish -p conductor-sdk --dry-run
cargo publish -p conductor-sdk
```

The SDK dry run cannot pass before the macros crate is indexed, because Cargo checks its
registry dependency. The root `Cargo.toml` already declares both the local path and the
published macros version; do not remove and restore the path dependency to publish.

After both crates are available, verify their versions on crates.io and compile the
README agent quickstart from a fresh Cargo project with
`conductor = { package = "conductor-sdk", version = "0.1.1", features = ["agents"] }`.
Then tag the released commit as `v0.1.1` and create the GitHub release with the
changelog notes.
