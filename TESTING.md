# Running Tests Against Conductor Server

## Quick Start

### 1. Start Conductor Server

**Option A: Docker (Recommended for testing)**
```bash
docker run -d \
  --name conductor \
  -p 8080:8080 \
  -p 5000:5000 \
  conductoross/conductor-standalone:latest
```

**Option B: Use existing server**
If you already have a Conductor instance running, skip to step 2.

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
docker run -d -p 8080:8080 conductoross/conductor-standalone:latest
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
# Stop and remove container
docker stop conductor
docker rm conductor
```

---

## Test Categories

### ✅ Always Work (OSS Conductor)
- `integration_tests.rs` - 17 tests (15 active, 2 ignored)
- `workflow_client_tests.rs` - 15 tests
- `task_client_tests.rs` - 15 tests (14 active, 1 ignored)
- `worker_tests.rs` - 10 tests
- `performance_test.rs` - 3 tests

**Total: ~60 tests** work with OSS Conductor

### 🔸 Require Orkes Conductor
- `orkes_client_tests.rs` - 16 tests (13 `#[ignore]`)
- `authorization_client_tests.rs` - 14 tests (11 `#[ignore]`)

### 📦 Library Unit Tests
- `src/` - 63 unit tests (always pass, no server required)

### 📖 Doc Tests  
- 10 doc tests (7 active, 3 ignored)

**To run Orkes tests:**
```bash
# Configure Orkes credentials first
export CONDUCTOR_SERVER_URL=https://your-instance.orkes.io/api
export CONDUCTOR_AUTH_KEY=your_key
export CONDUCTOR_AUTH_SECRET=your_secret

# Run ignored tests
cargo test --tests -- --ignored
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
docker logs conductor
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

For automated testing in CI:

```yaml
# .github/workflows/test.yml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    
    services:
      conductor:
        image: conductoross/conductor-standalone:latest
        ports:
          - 8080:8080
          
    steps:
      - uses: actions/checkout@v3
      
      - name: Wait for Conductor
        run: |
          timeout 60 bash -c 'until curl -s http://localhost:8080/health | grep -q healthy; do sleep 2; done'
      
      - name: Run tests
        env:
          CONDUCTOR_SERVER_URL: http://localhost:8080/api
        run: cargo test --tests
```

---

## Quick Reference

| Command | Purpose |
|---------|---------|
| `cargo test --tests` | Run all tests |
| `cargo test --test workflow_client_tests` | Run workflow tests |
| `cargo test test_start_workflow -- --exact` | Run single test |
| `cargo test -- --nocapture` | Show print statements |
| `cargo test -- --ignored` | Run Orkes-only tests |
| `RUST_LOG=debug cargo test` | Enable debug logs |

---

**You're ready to test! Start with the Quick Start section above.**
