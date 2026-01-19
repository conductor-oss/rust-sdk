# AGENTS.md - AI Agent Contribution Guidelines

This document provides guidelines for AI coding agents contributing to the Conductor Rust SDK.

## Project Overview

This is the official Rust SDK for [Netflix Conductor](https://conductor-oss.org/), a workflow orchestration engine. The SDK provides:

- **Workflow/Task Management**: Define and execute workflows
- **Worker Framework**: Build and run task workers  
- **Client APIs**: Full API coverage for Conductor server

## Quick Reference

| Component | Path | Purpose |
|-----------|------|---------|
| Library | `src/` | Core SDK implementation |
| Tests | `tests/` | Integration tests |
| Examples | `examples/` | Usage examples |
| Macros | `conductor-macros/` | Procedural macros |

## Build & Test Commands

```bash
# Build
cargo build

# Run all tests (requires Conductor server on localhost:8080)
cargo test

# Run specific test file
cargo test --test integration_tests

# Run single test
cargo test test_workflow_execution -- --exact

# Check for warnings (must pass with zero warnings)
cargo clippy --all-targets
```

## Environment Variables

```bash
# Required for integration tests
CONDUCTOR_SERVER_URL=http://localhost:8080/api

# Required for Orkes-specific tests
CONDUCTOR_AUTH_KEY=your_key
CONDUCTOR_AUTH_SECRET=your_secret
```

## Architecture Patterns

### Client Hierarchy

```
ConductorClient
├── metadata_client()        → MetadataClient (base CRUD)
├── orkes_metadata_client()  → OrkesMetadataClient (extends with tagging)
├── workflow_client()        → WorkflowClient
├── task_client()            → TaskClient
├── scheduler_client()       → SchedulerClient (Orkes)
├── secret_client()          → SecretClient (Orkes)
├── authorization_client()   → AuthorizationClient (Orkes)
├── prompt_client()          → PromptClient (Orkes)
└── event_client()           → EventClient
```

### Extension Pattern (Deref)

For Orkes-specific extensions, use composition + `Deref`:

```rust
pub struct OrkesMetadataClient {
    inner: MetadataClient,
    api: ApiClient,
}

impl Deref for OrkesMetadataClient {
    type Target = MetadataClient;
    fn deref(&self) -> &Self::Target { &self.inner }
}
```

### Model Definitions

Models are in `src/models/`. Use builder pattern:

```rust
let task = TaskDef::new("my_task")
    .with_description("Description")
    .with_retry(3, RetryLogic::Fixed, 10);
```

## Code Quality Standards

### Zero Warnings Policy

All code must compile with **zero warnings**. Check with:

```bash
cargo clippy --all-targets 2>&1 | grep -E "^warning:"
# Should produce no output
```

### Test Requirements

1. **All tests must pass**: `cargo test` should show 0 failures
2. **Ignored tests**: Use `#[ignore]` only for tests requiring special setup
3. **Cleanup**: Tests must clean up created resources

### Documentation

- All public APIs must have doc comments
- Examples should be functional (not `ignore` unless necessary)

## File Organization

```
src/
├── client/           # API clients
│   ├── mod.rs        # Exports
│   ├── conductor_client.rs
│   ├── metadata_client.rs
│   ├── orkes_metadata_client.rs  # Orkes extension
│   └── ...
├── models/           # Data structures
├── worker/           # Worker framework
├── http/             # HTTP client layer
├── error.rs          # Error types
└── lib.rs            # Library root

tests/
├── common/           # Shared test utilities
├── integration_tests.rs
├── workflow_client_tests.rs
├── task_client_tests.rs
└── ...
```

## Common Tasks

### Adding a New API Method

1. Add method to appropriate client in `src/client/`
2. Add corresponding test in `tests/`
3. Update any relevant examples

### Adding a New Model

1. Create struct in `src/models/`
2. Export in `src/models/mod.rs`
3. Use `#[derive(Debug, Clone, Serialize, Deserialize)]`
4. Use `#[serde(rename_all = "camelCase")]` for JSON fields

### Creating Orkes-Specific Extensions

1. Create new client file (e.g., `orkes_*_client.rs`)
2. Use `Deref` pattern to inherit from base client
3. Add `orkes_*_client()` method to `ConductorClient`
4. Export in `src/client/mod.rs`

## Testing Against Conductor

### Start Local Server

```bash
docker run -d -p 8080:8080 conductoross/conductor-standalone:latest
```

### Wait for Ready

```bash
while ! curl -s http://localhost:8080/health | grep -q "healthy"; do sleep 2; done
```

### Run Tests

```bash
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
cargo test --tests
```

## Key Files to Know

| File | Purpose |
|------|---------|
| `Cargo.toml` | Dependencies and metadata |
| `src/lib.rs` | Library entry point, public exports |
| `src/configuration.rs` | Client configuration |
| `tests/common/mod.rs` | Shared test utilities |
| `TESTING.md` | Detailed testing instructions |
| `README.md` | User documentation |

## Gotchas

1. **Serde naming**: Use `camelCase` for JSON, `snake_case` for Rust
2. **Optional fields**: Use `#[serde(skip_serializing_if = "Option::is_none")]`
3. **Timestamps**: Some APIs use ISO-8601, others use epoch milliseconds
4. **Search responses**: May contain stringified JSON that needs re-parsing
5. **Cleanup**: Always delete test resources in test cleanup sections
