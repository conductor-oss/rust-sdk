# Conductor Rust SDK - Test Suite

Comprehensive test suite for the Conductor Rust SDK, ported from the Java SDK test suite.

## Overview

This test suite includes 100+ tests covering:

- **Workflow Client** (15 tests) - Start, pause, resume, terminate, search, retry, bulk operations
- **Task Client** (15 tests) - Task updates, logging, queue operations, search, polling
- **Metadata / Integration Tests** - Task/workflow definitions, tagging, end-to-end workflow execution, system tasks
- **Worker Framework** (10 tests) - Polling, concurrency, error handling, configuration
- **Orkes Clients** (16 tests) - Scheduler, Secret, Prompt, Event clients
- **Authorization Client** (11 tests) - RBAC (applications, users, groups, permissions, roles)

## Test Organization

```
tests/
├── common/
│   └── mod.rs                      # Shared test utilities
├── workflow_client_tests.rs        # WorkflowClient tests
├── task_client_tests.rs            # TaskClient tests
├── integration_tests.rs            # Core integration tests + metadata
├── worker_tests.rs                 # Worker framework tests
├── orkes_client_tests.rs           # Scheduler/Secret/Prompt/Event client tests
├── authorization_client_tests.rs   # Authorization/RBAC tests
└── performance_test.rs             # Performance/load tests
```

## Gating Orkes-Enterprise-only tests: `ApiClient::is_oss()`

Rather than `#[ignore]` or a `conductor_available()` helper (neither exists in
this codebase), tests that only work against Orkes Enterprise Conductor call
`client.is_oss().await` at the top of the test body and return early if it's
`true`:

```rust
#[tokio::test]
async fn test_prompt_save_and_get() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Prompt API requires Orkes Enterprise Conductor");
        return;
    }
    // ... real assertions, not soft-`eprintln!`-and-continue ...
}
```

`is_oss()` probes `POST /token` once per client and caches the result: OSS
Conductor returns 404 (the endpoint doesn't exist), Enterprise returns a
non-404 (e.g. 401/403 for the dummy credentials used in the probe).

The following are empirically confirmed (as of this writing) to not exist on
plain OSS Conductor and are gated this way:

- `authorization_client_tests.rs` — the entire file (applications, users,
  groups, permissions, roles all 404 on OSS)
- `orkes_client_tests.rs` — the Prompt client tests and
  `test_event_queue_configuration`

Scheduler and event-handler tests in `orkes_client_tests.rs` *are*
OSS-compatible and run unconditionally against either server type.

Secret tests are a middle case: OSS Conductor registers a full secrets CRUD
controller by default (`conductor.integrations.ai.enabled=true` is the
default), but only ships read-only `SecretsDAO` backends. Reads (`get`,
`list`, `exists`) are asserted for real against an env-backed secret seeded
via `CONDUCTOR_SECRET_RUST_SDK_INTEGRATION_TEST` in
`scripts/docker-compose-oss.yaml`; writes (`put`/`delete`) are asserted to
fail with a real `501` rather than being skipped. This is safe against an
unauthenticated OSS server precisely because it's unauthenticated: OSS
Conductor has no auth at all (that's why the whole authorization surface
404s), so anyone who can already reach the REST API can do far more than read
a dummy seeded string — seeding a non-sensitive test value doesn't change
that threat model.

Whatever you gate this way, do it explicitly with a comment citing how you
confirmed it (e.g. "confirmed empirically: 404 ...") — don't reintroduce a
blanket try/`eprintln!`-and-continue pattern, since that silently swallows
real regressions against Enterprise too, not just OSS.

## Prerequisites

### For OSS Conductor Tests

The easiest way to get a local OSS Conductor + Postgres stack running is:

```bash
scripts/run-integration-oss.sh
```

This starts `scripts/docker-compose-oss.yaml`, waits for health, exports
`CONDUCTOR_SERVER_URL`, runs `cargo test --tests --all-features -- --test-threads=1`,
and tears the stack down on exit (`--keep-up` to leave it running).

To do it manually instead:

```bash
docker compose -f scripts/docker-compose-oss.yaml up -d
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```

### For Orkes Conductor Tests

1. **Orkes Cloud Account** or **Orkes Conductor Enterprise**

2. **Set Environment Variables**:
   ```bash
   export CONDUCTOR_SERVER_URL=https://play.orkes.io/api
   export CONDUCTOR_AUTH_KEY=your_key_id
   export CONDUCTOR_AUTH_SECRET=your_key_secret
   ```

## Running Tests

### Run All Tests

```bash
# Run all tests
cargo test --tests --all-features

# Run with serial execution (recommended for integration tests)
cargo test --tests --all-features -- --test-threads=1
```

### Run Specific Test File

```bash
# Workflow client tests
cargo test --test workflow_client_tests

# Task client tests
cargo test --test task_client_tests

# Worker tests
cargo test --test worker_tests

# Integration tests
cargo test --test integration_tests

# Orkes-only clients (self-skip against OSS via is_oss())
cargo test --test orkes_client_tests
cargo test --test authorization_client_tests
```

### Run Specific Test

```bash
# Run a single test
cargo test --test workflow_client_tests test_pause_workflow

# Run tests matching a pattern
cargo test --test workflow_client_tests pause
```

### Verbose Output

```bash
# Show test output
cargo test --tests -- --nocapture

# Show test output for specific test
cargo test --test workflow_client_tests test_pause_workflow -- --nocapture
```

## Test Utilities

The `common` module provides shared utilities:

```rust
use common::*;

// Configuration
let config = test_config();

// Generate unique names
let task_name = generate_unique_task_name("prefix");
let workflow_name = generate_unique_workflow_name("prefix");

// Retry with backoff (for eventual consistency)
retry_with_backoff(|| async {
    metadata.get_workflow_def(&name, Some(1)).await
}, 5).await?;
```

## Writing New Tests

### Test Template

```rust
#[tokio::test]
async fn test_my_feature() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let workflow_name = generate_unique_workflow_name("test_my_feature");

    // ... real assertions; let failures fail the test ...
}
```

### Orkes-Enterprise-only Test Template

```rust
#[tokio::test]
async fn test_orkes_only_feature() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: <feature> requires Orkes Enterprise Conductor");
        return;
    }

    // ... real assertions ...
}
```

## Continuous Integration

See the `test-integration` job in `.github/workflows/ci.yml`, which uses the
same `scripts/docker-compose-oss.yaml` stack as `scripts/run-integration-oss.sh`
(the OSS image tag is pinned via the `E2E_TEST_OSS_CONDUCTOR_VERSION`
organization variable, overridable via a `workflow_dispatch` input).

## Troubleshooting

### Connection refused

**Cause**: Wrong server URL, or the stack isn't up yet.

**Solution**: Check environment variable and health endpoint:
```bash
echo $CONDUCTOR_SERVER_URL
curl http://localhost:8080/health
```

### Tests timeout

**Cause**: Workflows taking longer than expected, or tests running concurrently
against shared server state.

**Solution**: Use `--test-threads=1`:
```bash
cargo test --tests -- --test-threads=1
```

### Authentication errors (Orkes tests)

**Cause**: Missing or invalid credentials.

**Solution**: Set auth credentials:
```bash
export CONDUCTOR_AUTH_KEY=your_key
export CONDUCTOR_AUTH_SECRET=your_secret
```

## Test Coverage

To generate a test coverage report:

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --tests --out Html --output-dir coverage

# Open report
open coverage/index.html
```

## Performance Tests

Run performance/load tests separately:

```bash
cargo test --test performance_test -- --nocapture
```

These tests measure:
- Worker throughput
- Concurrent workflow execution
- Polling performance
- Task update performance

## Contributing

When adding new tests:

1. Use unique names with `generate_unique_name()` to avoid conflicts
2. Always clean up resources in tests
3. Gate Orkes-Enterprise-only tests with an explicit `is_oss()` check and a
   comment explaining why (not a blanket try/`eprintln!`)
4. Add descriptive test names and comments
5. Follow the existing test patterns
