# Worker Configuration

The Conductor Rust SDK supports hierarchical worker configuration, allowing you to override worker settings at deployment time using environment variables without changing code.

## Configuration Hierarchy

Worker properties are resolved using a three-tier hierarchy (from lowest to highest priority):

1. **Code-level defaults** (lowest priority) - Values defined in worker builder methods or `#[worker]` macro
2. **Global worker config** (medium priority) - `CONDUCTOR_WORKER_ALL_<PROPERTY>` environment variables
3. **Worker-specific config** (highest priority) - `CONDUCTOR_WORKER_<WORKER_NAME>_<PROPERTY>` environment variables

This means:
- Worker-specific environment variables override everything
- Global environment variables override code defaults
- Code defaults are used when no environment variables are set

## Configurable Properties

The following properties can be configured via environment variables:

| Property | Type | Description | Example | In Code? |
|----------|------|-------------|---------|----------|
| `POLL_INTERVAL_MILLIS` | int | Polling interval in milliseconds | `1000` | Yes |
| `DOMAIN` | string | Worker domain for task routing | `production` | Yes |
| `WORKER_ID` | string | Unique worker identifier | `worker-1` | Yes |
| `THREAD_COUNT` | int | Max concurrent task executions | `10` | Yes |
| `REGISTER_TASK_DEF` | bool | Auto-register task definition on startup | `true` | Yes |
| `OVERWRITE_TASK_DEF` | bool | Overwrite existing task definitions when registering | `false` | Yes |
| `STRICT_SCHEMA` | bool | Enforce strict schema validation (additionalProperties=false) | `true` | Yes |
| `POLL_TIMEOUT` | int | Poll request timeout in milliseconds | `100` | Yes |
| `PAUSED` | bool | Pause worker from polling/executing tasks | `true` | **Env-only** |

**Notes**:
- The `PAUSED` property is intentionally **not available** in code. It can only be controlled via environment variables, allowing operators to pause/resume workers at runtime without code changes.
- The `REGISTER_TASK_DEF` parameter automatically registers task definitions with JSON Schema (draft-07) generated from Rust types using the `schemars` crate.
- The `STRICT_SCHEMA` parameter controls JSON schema validation strictness (default: false for lenient validation).

## Environment Variable Formats

The SDK supports two environment variable formats:

### Uppercase Format (Recommended)

```bash
# Global (all workers)
CONDUCTOR_WORKER_ALL_<PROPERTY>=<value>

# Worker-specific
CONDUCTOR_WORKER_<TASK_NAME>_<PROPERTY>=<value>
```

### Dot Notation Format (Python SDK Compatible)

```bash
# Global (all workers)
conductor.worker.all.<property>=<value>

# Worker-specific
conductor.worker.<task_name>.<property>=<value>
```

## Understanding `thread_count`

The `thread_count` parameter controls the maximum number of concurrent task executions. In the Rust SDK, this is implemented using a Tokio semaphore:

```rust
// thread_count = concurrency limit (semaphore permits)
let worker = FnWorker::new("api_task", handler)
    .with_thread_count(100);  // Up to 100 concurrent tasks

// All tasks run on the Tokio async runtime
// Recommended: 50-200 for I/O-bound tasks
```

**Performance Guidelines:**
- **I/O-bound tasks** (HTTP calls, database queries): Use higher values (50-200)
- **CPU-bound tasks**: Use lower values matching available CPU cores (1-8)
- **Mixed workloads**: Start with moderate values (10-20) and tune based on metrics

## Basic Example

### Code Definition

```rust
use conductor::worker::FnWorker;

let worker = FnWorker::new("process_order", |task| async move {
    let order_id: String = task.get_input("order_id").unwrap();
    Ok(WorkerOutput::completed_with_result(json!({"status": "processed"})))
})
.with_poll_interval_millis(1000)
.with_domain("dev")
.with_thread_count(5);
```

### Without Environment Variables

Worker uses code-level defaults:
- `poll_interval_millis=1000`
- `domain='dev'`
- `thread_count=5`

### With Global Override

```bash
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=500
export CONDUCTOR_WORKER_ALL_DOMAIN=production
```

Worker now uses:
- `poll_interval_millis=500` (from global env)
- `domain='production'` (from global env)
- `thread_count=5` (from code)

### With Worker-Specific Override

```bash
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=500
export CONDUCTOR_WORKER_ALL_DOMAIN=production
export CONDUCTOR_WORKER_PROCESS_ORDER_THREAD_COUNT=20
```

Worker now uses:
- `poll_interval_millis=500` (from global env)
- `domain='production'` (from global env)
- `thread_count=20` (from worker-specific env)

## Worker Definition Patterns

### Pattern 1: FnWorker (Function-based)

```rust
use conductor::worker::{FnWorker, WorkerOutput};

let worker = FnWorker::new("process_order", |task| async move {
    let order_id: String = task.get_input("order_id").unwrap();
    Ok(WorkerOutput::completed_with_result(json!({"status": "processed"})))
})
.with_poll_interval_millis(1000)
.with_domain("production")
.with_thread_count(10);
```

### Pattern 2: #[worker] Macro (Declarative)

Enable the `macros` feature in `Cargo.toml`:

```toml
[dependencies]
conductor = { version = "0.1", package = "conductor-sdk", features = ["macros"] }
```

```rust
use conductor_macros::worker;

#[worker(name = "process_order", poll_interval = 1000, domain = "production", thread_count = 10)]
async fn process_order(order_id: String, amount: f64) -> serde_json::Value {
    serde_json::json!({"status": "processed"})
}

// Use the generated function
handler.add_worker(process_order_worker());
```

### Pattern 3: Worker Trait Implementation

```rust
use async_trait::async_trait;
use conductor::{Worker, Task, error::Result, worker::WorkerOutput};

struct ProcessOrderWorker;

#[async_trait]
impl Worker for ProcessOrderWorker {
    fn task_definition_name(&self) -> &str { "process_order" }
    fn domain(&self) -> Option<&str> { Some("production") }
    fn poll_interval_millis(&self) -> u64 { 1000 }
    fn thread_count(&self) -> usize { 10 }
    
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let order_id: String = task.get_input("order_id").unwrap();
        Ok(WorkerOutput::completed_with_result(json!({"status": "processed"})))
    }
}
```

## Long-Running Tasks

For tasks that take longer than the poll interval, use `TaskInProgress` to extend the lease:

```rust
use conductor::worker::{WorkerOutput, TaskInProgress, TaskContext};

let worker = FnWorker::new("long_running_task", |task| async move {
    let ctx = TaskContext::from_task(&task);
    let poll_count = ctx.poll_count();
    
    // Process a chunk of work
    let progress = process_chunk(&task, poll_count).await;
    
    if progress < 100 {
        // More work to do - extend lease by returning TaskInProgress
        Ok(WorkerOutput::InProgress(
            TaskInProgress::new(60)  // Return to queue after 60 seconds
                .with_output_value("progress", progress)
        ))
    } else {
        // Done - return final result
        Ok(WorkerOutput::completed_with_result(json!({
            "status": "completed",
            "progress": 100
        })))
    }
});
```

## Understanding `overwrite_task_def`

Controls whether to overwrite existing task definitions when `register_task_def=true`:

**Overwrite Mode (default, `overwrite_task_def=true`):**
- Always updates task definitions on startup
- Ensures server always has latest configuration from code
- **Use when:** Task configuration changes frequently, development environments

**No-Overwrite Mode (`overwrite_task_def=false`):**
- Only creates new task if it doesn't exist
- Preserves manual changes made on server
- **Use when:** Tasks managed outside code, production with manual config

```bash
# Global: Never overwrite any task definitions
export CONDUCTOR_WORKER_ALL_OVERWRITE_TASK_DEF=false

# Specific: Allow overwrite for this worker only
export CONDUCTOR_WORKER_DYNAMIC_TASK_OVERWRITE_TASK_DEF=true
```

## Understanding `strict_schema`

Controls JSON Schema validation strictness when `register_task_def=true`:

**Lenient Mode (default, `strict_schema=false`):**
- Sets `additionalProperties=true` in schemas
- Allows extra fields beyond defined schema
- **Use when:** Backward compatibility, flexible integrations, development

**Strict Mode (`strict_schema=true`):**
- Sets `additionalProperties=false` in schemas
- Rejects inputs with extra fields
- **Use when:** Strict contract enforcement, production validation

```bash
# Global: Strict validation for all workers
export CONDUCTOR_WORKER_ALL_STRICT_SCHEMA=true

# Specific: Lenient for this worker (overrides global)
export CONDUCTOR_WORKER_FLEXIBLE_TASK_STRICT_SCHEMA=false
```

**Example Schemas:**

```json
// strict_schema=false (default)
{
  "type": "object",
  "properties": {"name": {"type": "string"}},
  "additionalProperties": true
}

// strict_schema=true
{
  "type": "object",
  "properties": {"name": {"type": "string"}},
  "additionalProperties": false
}
```

## JSON Schema Generation

Generate JSON Schemas from Rust types using the `schemars` crate:

```rust
use schemars::JsonSchema;
use conductor::schema::generate_schema;

#[derive(JsonSchema)]
struct OrderInput {
    order_id: String,
    amount: f64,
    items: Vec<String>,
}

#[derive(JsonSchema)]
struct OrderOutput {
    status: String,
    processed_at: String,
}

// Generate and attach schemas to worker
let worker = FnWorker::new("process_order", handler)
    .with_input_schema_from::<OrderInput>(true)   // strict=true
    .with_output_schema_from::<OrderOutput>(true);

// Or generate schema manually
let input_schema = generate_schema::<OrderInput>(true);
let output_schema = generate_schema::<OrderOutput>(true);

let worker = FnWorker::new("process_order", handler)
    .with_input_schema(input_schema)
    .with_output_schema(output_schema);
```

## Common Scenarios

### Production Deployment

Override all workers to use production domain and optimized settings:

```bash
# Global production settings
export CONDUCTOR_WORKER_ALL_DOMAIN=production
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250

# Critical worker needs more resources
export CONDUCTOR_WORKER_PROCESS_PAYMENT_THREAD_COUNT=50
export CONDUCTOR_WORKER_PROCESS_PAYMENT_POLL_INTERVAL_MILLIS=50
```

### Development/Debug Mode

Slow down polling for easier debugging:

```bash
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=10000  # 10 seconds
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=1              # Single concurrent task
```

### Staging Environment

Override only domain while keeping code defaults for other properties:

```bash
export CONDUCTOR_WORKER_ALL_DOMAIN=staging
```

### Pausing Workers

Temporarily disable workers without stopping the process:

```bash
# Pause all workers (maintenance mode)
export CONDUCTOR_WORKER_ALL_PAUSED=true

# Pause specific worker only
export CONDUCTOR_WORKER_PROCESS_ORDER_PAUSED=true
```

When a worker is paused:
- It stops polling for new tasks
- Already-executing tasks complete normally
- The `task_paused_total` metric is incremented for each skipped poll
- No code changes or process restarts required

**Unpause workers** by removing or setting the variable to false:
```bash
unset CONDUCTOR_WORKER_ALL_PAUSED
# or
export CONDUCTOR_WORKER_ALL_PAUSED=false
```

### Multi-Region Deployment

Route different workers to different regions using domains:

```bash
# US workers
export CONDUCTOR_WORKER_US_PROCESS_ORDER_DOMAIN=us-east
export CONDUCTOR_WORKER_US_PROCESS_PAYMENT_DOMAIN=us-east

# EU workers  
export CONDUCTOR_WORKER_EU_PROCESS_ORDER_DOMAIN=eu-west
export CONDUCTOR_WORKER_EU_PROCESS_PAYMENT_DOMAIN=eu-west
```

## Boolean Values

Boolean properties accept multiple formats:

**True values**: `true`, `1`, `yes`
**False values**: `false`, `0`, `no`

```bash
export CONDUCTOR_WORKER_ALL_REGISTER_TASK_DEF=true
export CONDUCTOR_WORKER_CRITICAL_TASK_STRICT_SCHEMA=1
export CONDUCTOR_WORKER_BACKGROUND_TASK_PAUSED=yes
```

## Docker/Kubernetes Example

### Docker Compose

```yaml
services:
  worker:
    image: my-conductor-worker
    environment:
      - CONDUCTOR_WORKER_ALL_DOMAIN=production
      - CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250
      - CONDUCTOR_WORKER_CRITICAL_TASK_THREAD_COUNT=50
```

### Kubernetes ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: worker-config
data:
  CONDUCTOR_WORKER_ALL_DOMAIN: "production"
  CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS: "250"
  CONDUCTOR_WORKER_CRITICAL_TASK_THREAD_COUNT: "50"
---
apiVersion: v1
kind: Pod
metadata:
  name: conductor-worker
spec:
  containers:
  - name: worker
    image: my-conductor-worker
    envFrom:
    - configMapRef:
        name: worker-config
```

### Kubernetes Deployment with Namespace-Based Config

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: conductor-worker-prod
  namespace: production
spec:
  template:
    spec:
      containers:
      - name: worker
        image: my-conductor-worker
        env:
        - name: CONDUCTOR_WORKER_ALL_DOMAIN
          value: "production"
        - name: CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS
          value: "250"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: conductor-worker-staging
  namespace: staging
spec:
  template:
    spec:
      containers:
      - name: worker
        image: my-conductor-worker
        env:
        - name: CONDUCTOR_WORKER_ALL_DOMAIN
          value: "staging"
        - name: CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS
          value: "500"
```

## Programmatic Access

You can also use the configuration resolver programmatically:

```rust
use conductor::configuration::{WorkerConfig, resolve_worker_config};

// Create defaults
let defaults = WorkerConfig::new("process_order")
    .with_poll_interval_millis(1000)
    .with_domain("dev")
    .with_thread_count(5);

// Resolve with environment variable overrides
let config = resolve_worker_config("process_order", defaults);

println!("Resolved config:");
println!("  poll_interval: {:?}", config.poll_interval);
println!("  domain: {:?}", config.domain);
println!("  thread_count: {}", config.thread_count);
println!("  paused: {}", config.paused);
```

## Best Practices

### 1. Use Global Config for Environment-Wide Settings

```bash
# Good: Set domain for entire environment
export CONDUCTOR_WORKER_ALL_DOMAIN=production

# Less ideal: Set for each worker individually
export CONDUCTOR_WORKER_WORKER1_DOMAIN=production
export CONDUCTOR_WORKER_WORKER2_DOMAIN=production
export CONDUCTOR_WORKER_WORKER3_DOMAIN=production
```

### 2. Use Worker-Specific Config for Exceptions

```bash
# Global settings for most workers
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=10
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250

# Exception: High-priority worker needs more resources
export CONDUCTOR_WORKER_CRITICAL_TASK_THREAD_COUNT=50
export CONDUCTOR_WORKER_CRITICAL_TASK_POLL_INTERVAL_MILLIS=50
```

### 3. Keep Code Defaults Sensible

Use sensible defaults in code so workers work without environment variables:

```rust
let worker = FnWorker::new("process_order", handler)
    .with_poll_interval_millis(1000)  // Reasonable default (1 second)
    .with_domain("dev")               // Safe default domain
    .with_thread_count(5);            // Moderate concurrency
```

### 4. Document Environment Variables

Maintain documentation of required environment variables for each deployment:

```markdown
# Production Environment Variables

## Required
- `CONDUCTOR_WORKER_ALL_DOMAIN=production`

## Optional (Recommended)
- `CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250`
- `CONDUCTOR_WORKER_ALL_THREAD_COUNT=20`

## Worker-Specific Overrides
- `CONDUCTOR_WORKER_CRITICAL_TASK_THREAD_COUNT=50`
- `CONDUCTOR_WORKER_CRITICAL_TASK_POLL_INTERVAL_MILLIS=50`
```

### 5. Use Infrastructure as Code

Manage environment variables through IaC tools:

```hcl
# Terraform example
resource "kubernetes_deployment" "worker" {
  spec {
    template {
      spec {
        container {
          env {
            name  = "CONDUCTOR_WORKER_ALL_DOMAIN"
            value = var.environment_name
          }
          env {
            name  = "CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS"
            value = var.worker_poll_interval_millis
          }
          env {
            name  = "CONDUCTOR_WORKER_ALL_THREAD_COUNT"
            value = var.worker_thread_count
          }
        }
      }
    }
  }
}
```

## Troubleshooting

### Configuration Not Applied

**Problem**: Environment variables don't seem to take effect

**Solutions**:
1. Check environment variable names are correctly formatted:
   - Global: `CONDUCTOR_WORKER_ALL_<PROPERTY>`
   - Worker-specific: `CONDUCTOR_WORKER_<TASK_NAME>_<PROPERTY>`
   - Task name is converted to UPPERCASE with dashes replaced by underscores

2. Verify the task definition name matches exactly:
```rust
// Task name is "process-order"
let worker = FnWorker::new("process-order", handler);
```
```bash
# Environment variable uses PROCESS_ORDER (uppercase, dash -> underscore)
export CONDUCTOR_WORKER_PROCESS_ORDER_DOMAIN=production
```

3. Check environment variables are exported and visible:
```bash
env | grep CONDUCTOR_WORKER
```

### Boolean Values Not Parsed Correctly

**Problem**: Boolean properties not behaving as expected

**Solution**: Use recognized boolean values (case-insensitive):
```bash
# All of these work for true
export CONDUCTOR_WORKER_ALL_PAUSED=true
export CONDUCTOR_WORKER_ALL_PAUSED=TRUE
export CONDUCTOR_WORKER_ALL_PAUSED=1
export CONDUCTOR_WORKER_ALL_PAUSED=yes

# All of these work for false
export CONDUCTOR_WORKER_ALL_PAUSED=false
export CONDUCTOR_WORKER_ALL_PAUSED=0
export CONDUCTOR_WORKER_ALL_PAUSED=no
```

### Integer Values Not Parsed

**Problem**: Integer properties cause errors or use default

**Solution**: Ensure values are valid integers:
```bash
# Correct
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=10
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=500

# Incorrect (will use default)
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=ten
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=0.5
```

## Summary

The hierarchical worker configuration system provides flexibility to:

- **Deploy once, configure anywhere**: Same code works in dev/staging/prod
- **Override at runtime**: No code changes needed for environment-specific settings
- **Fine-tune per worker**: Optimize critical workers without affecting others
- **Simplify management**: Use global settings for common configurations
- **Pause/resume at runtime**: Control worker execution without redeployment

**Configuration priority**: Worker-specific > Global > Code defaults

### Quick Reference

| Environment Variable | Description |
|---------------------|-------------|
| `CONDUCTOR_WORKER_ALL_DOMAIN` | Domain for all workers |
| `CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS` | Poll interval for all workers |
| `CONDUCTOR_WORKER_ALL_THREAD_COUNT` | Concurrency for all workers |
| `CONDUCTOR_WORKER_ALL_PAUSED` | Pause all workers |
| `CONDUCTOR_WORKER_<NAME>_DOMAIN` | Domain for specific worker |
| `CONDUCTOR_WORKER_<NAME>_POLL_INTERVAL_MILLIS` | Poll interval for specific worker |
| `CONDUCTOR_WORKER_<NAME>_THREAD_COUNT` | Concurrency for specific worker |
| `CONDUCTOR_WORKER_<NAME>_PAUSED` | Pause specific worker |
| `CONDUCTOR_WORKER_ALL_REGISTER_TASK_DEF` | Auto-register task definitions |
| `CONDUCTOR_WORKER_ALL_OVERWRITE_TASK_DEF` | Overwrite existing task definitions |
| `CONDUCTOR_WORKER_ALL_STRICT_SCHEMA` | Strict JSON Schema validation |

---

## Related Documentation

- **[DESIGN.md](DESIGN.md)** - Complete SDK architecture and API documentation
- **[WORKER_COMPARISON.md](WORKER_COMPARISON.md)** - Feature comparison with Python SDK
- **[examples/worker_config_example.rs](examples/worker_config_example.rs)** - Working example

---

**Last Updated**: 2025-01-19
