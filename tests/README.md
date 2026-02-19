# Conductor Rust SDK - Test Suite

Comprehensive test suite for the Conductor Rust SDK, ported from the Java SDK test suite.

## Overview

This test suite includes **100+ tests** covering:

- **Workflow Client** (23 tests) - Start, pause, resume, terminate, search, retry, bulk operations
- **Task Client** (15 tests) - Task updates, logging, queue operations, search, polling
- **Metadata Client** (4 tests) - Task/workflow definitions, tagging
- **Worker Framework** (8 tests) - Polling, concurrency, error handling, configuration
- **Integration Tests** (30+ tests) - End-to-end workflow execution, system tasks
- **Orkes Clients** (30+ test stubs) - Scheduler, Secret, Prompt, Event, Authorization

## Test Organization

```
tests/
├── common/
│   └── mod.rs                      # Shared test utilities
├── workflow_client_tests.rs        # WorkflowClient tests
├── task_client_tests.rs            # TaskClient tests
├── integration_tests.rs            # Core integration tests + metadata
├── worker_tests.rs                 # Worker framework tests
├── orkes_client_tests.rs           # Orkes-specific client tests
├── authorization_client_tests.rs   # Authorization/RBAC tests
└── performance_test.rs             # Performance/load tests
```

## Prerequisites

### For OSS Conductor Tests

1. **Start Conductor Server**:
   ```bash
   docker run --init -p 8080:8080 -p 5000:5000 \
     conductoross/conductor:latest
   ```

2. **Set Environment Variables**:
   ```bash
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
# Run all non-ignored tests
cargo test --tests

# Run with serial execution (recommended for integration tests)
cargo test --tests -- --test-threads=1
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
```

### Run Specific Test

```bash
# Run a single test
cargo test --test workflow_client_tests test_pause_workflow

#Run tests matching a pattern
cargo test --test workflow_client_tests pause
```

### Run Ignored Tests (Orkes-specific)

```bash
# Run all ignored tests
cargo test --tests -- --ignored

# Run specific ignored test file
cargo test --test orkes_client_tests -- --ignored

# Run both normal and ignored tests
cargo test --tests -- --include-ignored
```

### Verbose Output

```bash
# Show test output
cargo test --tests -- --nocapture

# Show test output for specific test
cargo test --test workflow_client_tests test_pause_workflow -- --nocapture
```

## Test Categories

### ✅ Always Run (OSS Conductor)

These tests work with the open-source Conductor server:

- `workflow_client_tests.rs` - All workflow operations
- `task_client_tests.rs` - All task operations
- `integration_tests.rs` - Core integration tests
- `worker_tests.rs` - Worker framework tests

### ⚠️ Requires Orkes (Marked `#[ignore]`)

These tests require Orkes Conductor features:

- `orkes_client_tests.rs` - Scheduler, Secret, Prompt, Event clients
- `authorization_client_tests.rs` - RBAC features
- Tagged operations in `integration_tests.rs`

## Test Utilities

The `common` module provides shared utilities:

```rust
use common::*;

// Configuration
let config = test_config();

// Check if Conductor is available
if !conductor_available().await {
    return; // Skip test
}

// Generate unique names
let task_name = generate_unique_task_name("prefix");
let workflow_name = generate_unique_workflow_name("prefix");

// Cleanup helpers
cleanup_workflow(&client, &workflow_id).await;
cleanup_task_def(&client, &task_name).await;
cleanup_workflow_def(&client, &workflow_name, 1).await;

// Retry with backoff (for eventual consistency)
retry_with_backoff(|| async {
    metadata.get_workflow_def(&name, Some(1)).await
}, 5).await?;

// Wait for workflow status
wait_for_workflow_status(&client, &workflow_id, WorkflowStatus::Completed, Duration::from_secs(30)).await?;
```

## Writing New Tests

### Test Template

```rust
#[tokio::test]
async fn test_my_feature() {
    // 1. Check availability
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    // 2. Setup
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let workflow_name = generate_unique_workflow_name("test_my_feature");

    // 3. Test logic
    // ... your test code ...

    // 4. Cleanup
    cleanup_workflow_def(&client, &workflow_name, 1).await;
}
```

### Orkes-Specific Test Template

```rust
#[tokio::test]
#[ignore] // Requires Orkes Conductor
async fn test_orkes_feature() {
    if !conductor_available().await {
        return;
    }

    // ... test logic ...
}
```

## Expected Test Results

### Passing Tests (OSS Conductor)

With a running OSS Conductor server, you should see:

```
running 80 tests
test test_task_def_crud ... ok
test test_workflow_def_crud ... ok
test test_pause_workflow ... ok
test test_worker_poll_and_execute ... ok
...

test result: ok. 80 passed; 0 failed; 30 ignored; 0 measured
```

### Ignored Tests

Tests marked `#[ignore]` will show:

```
test test_scheduler_save_and_get ... ignored
test test_secret_put_and_get ... ignored
...
```

To run them:
```bash
cargo test --tests -- --ignored
```

## Continuous Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    services:
      conductor:
        image: conductoross/conductor:latest
        ports:
          - 8080:8080
          - 5000:5000
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Wait for Conductor
        run: |
          timeout 60 bash -c 'until curl -f http://localhost:8080/health; do sleep 2; done'
      
      - name: Run tests
        run: cargo test --tests -- --test-threads=1
        env:
          CONDUCTOR_SERVER_URL: http://localhost:8080/api
```

## Troubleshooting

### Tests are skipped

**Cause**: Conductor server not available

**Solution**:
```bash
# Check if Conductor is running
curl http://localhost:8080/health

# Start Conductor if not running
docker run --init -p 8080:8080 -p 5000:5000 \
  conductoross/conductor:latest
```

### Tests timeout

**Cause**: Workflows taking longer than expected

**Solution**: Increase timeout or use `--test-threads=1`:
```bash
cargo test --tests -- --test-threads=1
```

### Connection refused

**Cause**: Wrong server URL

**Solution**: Check environment variable:
```bash
echo $CONDUCTOR_SERVER_URL
# Should be: http://localhost:8080/api
```

### Authentication errors (Orkes tests)

**Cause**: Missing or invalid credentials

**Solution**: Set auth credentials:
```bash
export CONDUCTOR_AUTH_KEY=your_key
export CONDUCTOR_AUTH_SECRET=your_secret
```

## Test Coverage

To generate test coverage report:

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
2. Always cleanup resources in tests
3. Mark Orkes-specific tests with `#[ignore]`
4. Add descriptive test names and comments
5. Follow the existing test patterns

## Summary

- **OSS Tests**: ~80 tests for core functionality
- **Orkes Tests**: ~30 test stubs (require Orkes Conductor)  
- **Total Coverage**: ~110 tests across all clients and features
- **Easy to Run**: `cargo test --tests`
- **CI-Ready**: Works in GitHub Actions and other CI systems
