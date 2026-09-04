# Running Tests Against Conductor Server

## Quick Start

### Option A: One-shot local OSS run (recommended)

```bash
scripts/run-integration-oss.sh
```

This starts a Postgres-backed Conductor OSS stack (`scripts/docker-compose-oss.yaml`),
waits for it to be healthy, exports `CONDUCTOR_SERVER_URL`, runs the full test
suite, and tears the stack down on exit. Pass `--version <tag>` to pin a
specific `conductoross/conductor` image, or `--keep-up` to leave the stack
running afterwards for manual poking.

### Option B: Manual setup

#### 1. Start Conductor Server

```bash
docker compose -f scripts/docker-compose-oss.yaml up -d
```

(Or, for Orkes Conductor, skip straight to step 2 with your existing instance.)

### 2. Configure Environment

Create a `.env` file in the repo root or export these variables:

```bash
# For OSS Conductor (no auth)
export CONDUCTOR_SERVER_URL=http://localhost:8080/api

# For Orkes Conductor (with auth)
export CONDUCTOR_SERVER_URL=https://your-instance.orkes.io/api
export CONDUCTOR_AUTH_KEY=your_key_id
export CONDUCTOR_AUTH_SECRET=your_secret
```

### 3. Verify Server is Running

```bash
# Test connection
curl http://localhost:8080/health

# Should return: {"healthy":true}
```

### 4. Run Tests

**Run all tests:**
```bash
cd /Users/viren/workspace/github/orkes/sdk/rust-sdk
cargo test --tests
```

**Run specific test file:**
```bash
# Workflow tests
cargo test --test workflow_client_tests

# Task tests  
cargo test --test task_client_tests

# Integration tests
cargo test --test integration_tests
```

**Run a single test:**
```bash
cargo test --test workflow_client_tests test_start_workflow -- --exact
```

**Run with verbose output:**
```bash
cargo test --test workflow_client_tests -- --nocapture
```

---

## Expected Results

### Successful Test Run
```
running 18 tests
test test_start_workflow ... ok
test test_terminate_workflow ... ok
test test_pause_workflow ... ok
...
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured
```

### Common Issues

**Server not available:**
```
test test_start_workflow ... FAILED
Error: Connection refused
```
**Fix:** Ensure Conductor server is running on port 8080

**Auth failure (Orkes only):**
```
Error: Unauthorized: Invalid token
```
**Fix:** Check your CONDUCTOR_AUTH_KEY and CONDUCTOR_AUTH_SECRET

---

## Manual Testing Workflow

### 1. Start server
```bash
docker compose -f scripts/docker-compose-oss.yaml up -d
```

### 2. Wait for server to be ready (~30 seconds)
```bash
# Keep checking until healthy
while ! curl -s http://localhost:8080/health | grep -q "healthy"; do
  echo "Waiting for Conductor..."
  sleep 2
done
echo "Conductor is ready!"
```

### 3. Run tests
```bash
# Set URL
export CONDUCTOR_SERVER_URL=http://localhost:8080/api

# Run a quick smoke test
cargo test --test workflow_client_tests test_start_workflow -- --exact --nocapture
```

### 4. Verify in Conductor UI
Open http://localhost:8080 in your browser to see:
- Workflows that were created
- Tasks that were executed  
- Definitions that were registered

### 5. Cleanup
```bash
# Stop and remove the stack (including the Postgres volume)
docker compose -f scripts/docker-compose-oss.yaml down -v
```

---

## Test Categories

### ✅ Always Run (OSS-compatible)
- `integration_tests.rs`, `workflow_client_tests.rs`, `task_client_tests.rs`, `worker_tests.rs`, `performance_test.rs`
- `orkes_client_tests.rs`'s Scheduler and Event-handler tests (confirmed empirically to work against plain OSS Conductor)
- `orkes_client_tests.rs`'s Secret *read* tests (`get`/`list`/`exists`) -- asserted against an
  env-backed secret seeded via `CONDUCTOR_SECRET_RUST_SDK_INTEGRATION_TEST` in
  `scripts/docker-compose-oss.yaml`. Secret *writes* (`put`/`delete`) still can't succeed against
  OSS's bundled read-only backends, but are asserted to fail with a real `501` rather than skipped.

### 🔸 Require Orkes Enterprise Conductor (self-skip via `ApiClient::is_oss()`)
- `authorization_client_tests.rs` - the whole file (applications, users, groups, permissions, roles)
- `orkes_client_tests.rs` - Prompt client tests, plus `test_event_queue_configuration`

These are gated with an explicit `if client.is_oss().await { println!("Skipping: ..."); return; }`
check rather than `#[ignore]` — see `tests/README.md` for the full explanation
and rationale. When run against real Orkes Enterprise Conductor
(`is_oss() == false`), these tests execute and assert for real.

**Why seeding a secret into an unauthenticated OSS server is fine:** OSS Conductor has no
authentication/authorization at all (that's exactly why the authorization tests above 404 on it),
so an unauthenticated read of a dummy, non-sensitive seeded value doesn't introduce any new
exposure -- anyone who can already reach the REST API has far more access than that.

### 📦 Library Unit Tests
- `src/` unit tests (always pass, no server required)

**To run against Orkes Enterprise:**
```bash
# Configure Orkes credentials first
export CONDUCTOR_SERVER_URL=https://your-instance.orkes.io/api
export CONDUCTOR_AUTH_KEY=your_key
export CONDUCTOR_AUTH_SECRET=your_secret

cargo test --tests --all-features -- --test-threads=1
```

---

## Debugging Failed Tests

### Enable debug logging
```bash
export RUST_LOG=conductor=debug
cargo test --test workflow_client_tests -- --nocapture
```

### Check server logs
```bash
docker compose -f scripts/docker-compose-oss.yaml logs -f conductor-server
```

### Test individual operations
```bash
# Create a simple workflow via API
curl -X POST http://localhost:8080/api/metadata/workflow \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test_workflow",
    "version": 1,
    "tasks": []
  }'
```

---

## Performance Testing

```bash
# Run performance test
cargo test --test performance_test -- --nocapture

# Should show:
# - Workflow execution times
# - Task processing rates
# - API latency metrics
```

---

## CI/CD Integration

See the `integration-tests-oss` job in [.github/workflows/ci.yml](.github/workflows/ci.yml)
and the `test` job in [.github/workflows/publish.yml](.github/workflows/publish.yml),
both of which start the same `scripts/docker-compose-oss.yaml` stack used by
`scripts/run-integration-oss.sh` locally. The OSS image tag is pinned via the
`E2E_TEST_OSS_CONDUCTOR_VERSION` organization variable (overridable in
`ci.yml` via a `workflow_dispatch` input).

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `cargo test --tests` | Run all tests |
| `cargo test --test workflow_client_tests` | Run workflow tests |
| `cargo test test_start_workflow -- --exact` | Run single test |
| `cargo test -- --nocapture` | Show print statements |
| `cargo test --tests --all-features -- --nocapture` | Show `Skipping: ...` gate messages |
| `RUST_LOG=debug cargo test` | Enable debug logs |

---

**You're ready to test! Start with the Quick Start section above.**
