# Upgrading the Rust SDK Safely

Before upgrading, read the release notes for breaking changes and test against the target
Conductor server version in a staging environment first.

```shell
cargo update -p conductor-sdk --precise <new-version>
cargo test --all-features
```

Run this crate's own test suite plus one representative workflow and (if used) agent execution
against your actual server before rolling out to production — see
[TESTING.md](TESTING.md).

Roll back by restoring the prior version in `Cargo.lock` (`cargo update -p conductor-sdk
--precise <prior-version>`) rather than changing active workflow behavior in place; keep workflow
definitions versioned (see [WORKFLOW_LIFECYCLE.md](WORKFLOW_LIFECYCLE.md)) so a rollback doesn't
also require rolling back server-side definitions.

This crate is pre-1.0 (`0.x`); expect breaking changes between minor versions until a 1.0
release, and pin an exact version in `Cargo.toml` rather than a caret range if that matters for
your deployment.

## Related Documentation

- **[CHANGELOG.md](CHANGELOG.md)**
- **[TESTING.md](TESTING.md)**
- **[WORKFLOW_LIFECYCLE.md](WORKFLOW_LIFECYCLE.md)**
