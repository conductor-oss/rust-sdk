# Publishing Guide

This document provides instructions for publishing the Conductor Rust SDK to [crates.io](https://crates.io).

## Table of Contents

- [Prerequisites](#prerequisites)
- [One-Time Setup](#one-time-setup)
- [Pre-Publish Checklist](#pre-publish-checklist)
- [Publishing Process](#publishing-process)
- [Versioning Strategy](#versioning-strategy)
- [Troubleshooting](#troubleshooting)
- [CI/CD Automation](#cicd-automation)

---

## Prerequisites

1. **Rust toolchain** (1.75+)
   ```shell
   rustup update stable
   ```

2. **crates.io account**
   - Create an account at https://crates.io (via GitHub login)
   - You must be an owner of the crates or have publish permissions

3. **API token**
   - Generate at https://crates.io/settings/tokens
   - Store securely (needed for publishing)

---

## One-Time Setup

### 1. Login to crates.io

```shell
cargo login <your-api-token>
```

This stores the token in `~/.cargo/credentials.toml`.

### 2. Add LICENSE file

A LICENSE file is **required** for publishing. Create it in the repository root:

```shell
curl -o LICENSE https://www.apache.org/licenses/LICENSE-2.0.txt
```

Or create `LICENSE` manually with the Apache 2.0 license text.

### 3. Verify Cargo.toml metadata

Both crates need proper metadata. The current configuration already includes:

**conductor-rust (main crate):**
```toml
[package]
name = "conductor-rust"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "Rust SDK for Netflix Conductor workflow orchestration"
license = "Apache-2.0"
repository = "https://github.com/conductor-oss/conductor-rust"
keywords = ["conductor", "workflow", "orchestration", "microservices"]
categories = ["api-bindings", "asynchronous"]
readme = "README.md"
```

**conductor-macros (proc-macro crate):**
```toml
[package]
name = "conductor-macros"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "Procedural macros for Conductor Rust SDK"
license = "Apache-2.0"
repository = "https://github.com/conductor-oss/conductor-rust"
```

### 4. Add missing recommended fields (optional but recommended)

Consider adding these to `Cargo.toml`:

```toml
[package]
# ... existing fields ...
authors = ["Orkes Inc. <info@orkes.io>"]
documentation = "https://docs.rs/conductor-rust"
homepage = "https://conductor-oss.org"
exclude = [
    ".github/*",
    ".gitignore",
    "PUBLISHING.md",
    "DESIGN.md",
    "SDK_COMPARISON.md",
    "WORKER_COMPARISON.md",
    "EXAMPLES_COMPARISON.md",
    "launch_blog.md",
]
```

---

## Pre-Publish Checklist

Run through this checklist before every release:

### 1. Verify license headers

All source files must have the license header from `licenseheader.txt`:

```shell
# Check for missing headers
./scripts/add-license-headers.sh --check

# Add/update headers if needed
./scripts/add-license-headers.sh
```

### 2. Ensure all tests pass

```shell
# Run all tests
cargo test --all-features

# Run clippy (must have zero errors)
cargo clippy --all-features --all-targets -- -D warnings

# Check formatting
cargo fmt --check
```

### 3. Verify examples compile

```shell
cargo check --examples
```

### 4. Verify documentation builds

```shell
cargo doc --no-deps --all-features
```

### 5. Dry-run the publish

```shell
# For main crate
cargo publish --dry-run

# For macros crate
cargo publish --dry-run -p conductor-macros
```

### 6. Update version numbers

Update version in **both** `Cargo.toml` files:
- `Cargo.toml` (root)
- `conductor-macros/Cargo.toml`

Also update the dependency version in the main crate:
```toml
# In root Cargo.toml
conductor-macros = { path = "conductor-macros", version = "0.1.0", optional = true }
```

### 7. Update CHANGELOG (if exists)

Document changes for the new version.

### 8. Commit and tag

```shell
git add -A
git commit -m "Release v0.1.0"
git tag v0.1.0
git push origin main --tags
```

---

## Publishing Process

### Order matters!

The `conductor-macros` crate must be published **before** the main `conductor-rust` crate because the main crate depends on it.

### Step 1: Publish conductor-macros

```shell
cd conductor-macros
cargo publish
cd ..
```

Wait 1-2 minutes for the crate to be indexed on crates.io.

### Step 2: Update main crate dependency

Before publishing the main crate, update the dependency to use the published version:

```toml
# In root Cargo.toml, change:
conductor-macros = { path = "conductor-macros", optional = true }

# To:
conductor-macros = { version = "0.1.0", optional = true }
```

### Step 3: Publish conductor-rust

```shell
cargo publish
```

### Step 4: Revert to path dependency (for development)

After publishing, revert the dependency for local development:

```toml
conductor-macros = { path = "conductor-macros", version = "0.1.0", optional = true }
```

---

## Versioning Strategy

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): Breaking API changes
- **MINOR** (0.2.0): New features, backward compatible
- **PATCH** (0.1.1): Bug fixes, backward compatible

### Pre-1.0 conventions

While version is < 1.0.0:
- MINOR bumps may include breaking changes
- PATCH bumps are for bug fixes only

### Version synchronization

Keep `conductor-rust` and `conductor-macros` versions in sync:
- Both at `0.1.0`, `0.2.0`, etc.
- Publish both even if only one changed (for consistency)

---

## Troubleshooting

### Error: "crate version already exists"

You cannot overwrite a published version. Bump the version number and publish again.

### Error: "no crate with name conductor-macros"

The dependency crate hasn't been indexed yet. Wait 1-2 minutes and retry.

### Error: "missing LICENSE file"

Add a LICENSE file to the repository root:
```shell
curl -o LICENSE https://www.apache.org/licenses/LICENSE-2.0.txt
```

### Error: "failed to verify package"

Run `cargo package --list` to see what's included. Common issues:
- Missing files referenced in Cargo.toml
- Files excluded that shouldn't be

### Error: "readme not found"

Ensure `readme = "README.md"` in Cargo.toml and the file exists.

### Error: "unauthorized"

Re-authenticate:
```shell
cargo logout
cargo login <your-new-token>
```

### Build fails on crates.io

The crate is built on crates.io's infrastructure. Ensure:
- No path dependencies (except for workspace members)
- No git dependencies in published crates
- All features work independently

---

## CI/CD Automation

### GitHub Actions workflow for publishing

Create `.github/workflows/publish.yml`:

```yaml
name: Publish to crates.io

on:
  release:
    types: [published]

env:
  CARGO_TERM_COLOR: always

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-action@stable

      - name: Verify version matches tag
        run: |
          TAG_VERSION=${GITHUB_REF#refs/tags/v}
          CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
          if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "Tag version ($TAG_VERSION) doesn't match Cargo.toml version ($CARGO_VERSION)"
            exit 1
          fi

      - name: Check license headers
        run: ./scripts/add-license-headers.sh --check

      - name: Run tests
        run: cargo test --all-features

      - name: Run clippy
        run: cargo clippy --all-features --all-targets -- -D warnings

      - name: Publish conductor-macros
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          cd conductor-macros
          cargo publish --token $CARGO_REGISTRY_TOKEN
          cd ..
          sleep 60  # Wait for crates.io indexing

      - name: Update dependency to published version
        run: |
          VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
          sed -i 's|conductor-macros = { path = "conductor-macros"|conductor-macros = { version = "'$VERSION'"|' Cargo.toml

      - name: Publish conductor-rust
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish --token $CARGO_REGISTRY_TOKEN
```

### Setting up the secret

1. Generate an API token at https://crates.io/settings/tokens
2. In GitHub repository settings, go to **Secrets and variables** > **Actions**
3. Add a new secret named `CARGO_REGISTRY_TOKEN` with your token

### Release process with automation

1. Update version numbers in both `Cargo.toml` files
2. Commit changes: `git commit -am "Bump version to X.Y.Z"`
3. Create a GitHub release with tag `vX.Y.Z`
4. The workflow automatically publishes both crates

---

## Quick Reference

### License header commands

```shell
# Check for missing headers (CI mode - fails if missing)
./scripts/add-license-headers.sh --check

# Add/update headers in all files
./scripts/add-license-headers.sh
```

### Publish commands

```shell
# Dry run (verify everything)
cargo publish --dry-run
cargo publish --dry-run -p conductor-macros

# Actual publish
cargo publish -p conductor-macros  # First!
cargo publish                       # Second!
```

### Useful links

- **crates.io dashboard**: https://crates.io/crates/conductor-rust
- **docs.rs**: https://docs.rs/conductor-rust
- **API tokens**: https://crates.io/settings/tokens
- **Publishing guide**: https://doc.rust-lang.org/cargo/reference/publishing.html

### Crate ownership

To add another owner (for team publishing):

```shell
cargo owner --add <github-username> conductor-rust
cargo owner --add <github-username> conductor-macros
```

---

## Post-Publish Verification

After publishing, verify:

1. **Crate is visible**: https://crates.io/crates/conductor-rust
2. **Docs are generated**: https://docs.rs/conductor-rust (may take a few minutes)
3. **Install works**:
   ```shell
   cargo new test-conductor && cd test-conductor
   cargo add conductor-rust
   cargo build
   ```

Congratulations! Your crate is now published! 🎉
