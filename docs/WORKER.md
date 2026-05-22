# Worker Implementation Guide

This guide covers how to implement task workers in the Conductor Rust SDK. Workers are the components that execute tasks in your workflows.

## Table of Contents

- [Quick Start](#quick-start)
- [Worker Types](#worker-types)
- [TaskHandler](#taskhandler)
- [Worker Configuration](#worker-configuration)
- [Error Handling](#error-handling)
- [Long-Running Tasks](#long-running-tasks)
- [Metrics and Events](#metrics-and-events)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, TaskHandler, FnWorker, WorkerOutput};
use conductor::models::Task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("http://localhost:8080/api");

    // Create a simple worker
    let worker = FnWorker::new("greet_task", |task: Task| async move {
        let name: String = task.get_input("name").unwrap_or_else(|| "World".to_string());
        Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", name)))
    });

    // Start the handler
    let mut handler = TaskHandler::new(config)?;
    handler.add_worker(worker);
    handler.start().await?;

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    handler.stop().await?;

    Ok(())
}
```

---

## Worker Types

### FnWorker - Function-Based Worker

The simplest way to create a worker using a closure:

```rust
use conductor::{FnWorker, WorkerOutput};
use conductor::models::Task;

let worker = FnWorker::new("my_task", |task: Task| async move {
    // Access task inputs
    let value: i32 = task.get_input("value").unwrap_or(0);
    
    // Process the task
    let result = value * 2;
    
    // Return the result
    Ok(WorkerOutput::completed_with_result(result))
})
.with_thread_count(10)          // Concurrent executions
.with_poll_interval_millis(100) // Polling frequency
.with_domain("production");     // Task routing domain
```

### FnWorkerArc - High-Performance Worker

For high-throughput scenarios, use `FnWorkerArc` which avoids cloning the task:

```rust
use conductor::{FnWorkerArc, WorkerOutput};
use conductor::models::Task;
use std::sync::Arc;

let worker = FnWorkerArc::new("high_throughput_task", |task: Arc<Task>| async move {
    // Task is passed as Arc, no cloning needed
    let data: String = task.get_input("data").unwrap_or_default();
    
    // Process efficiently
    Ok(WorkerOutput::completed_with_result(data.len()))
})
.with_thread_count(50)
.with_poll_interval_millis(10);
```

### Worker Trait - Custom Implementation

For complex workers, implement the `Worker` trait:

```rust
use async_trait::async_trait;
use conductor::worker::{Worker, WorkerOutput};
use conductor::models::Task;
use conductor::error::Result;

struct MyWorker {
    db_pool: DatabasePool,
}

#[async_trait]
impl Worker for MyWorker {
    fn task_definition_name(&self) -> &str {
        "database_task"
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let query: String = task.get_input("query").unwrap_or_default();
        
        // Use shared resources
        let result = self.db_pool.execute(&query).await?;
        
        Ok(WorkerOutput::completed_with_result(result))
    }

    fn thread_count(&self) -> usize {
        20
    }

    fn poll_interval_millis(&self) -> u64 {
        50
    }

    fn domain(&self) -> Option<&str> {
        Some("database")
    }
}
```

---

## TaskHandler

The `TaskHandler` manages the lifecycle of workers:

### Basic Usage

```rust
use conductor::{Configuration, TaskHandler, FnWorker};

let config = Configuration::new("http://localhost:8080/api");
let mut handler = TaskHandler::new(config)?;

// Add workers
handler.add_worker(worker1);
handler.add_worker(worker2);

// Start all workers
handler.start().await?;

// ... application runs ...

// Graceful shutdown (waits for in-flight tasks)
handler.stop().await?;
```

### Builder Pattern

```rust
use conductor::{TaskHandler, Configuration, MetricsSettings};

let handler = TaskHandler::builder(config)
    .worker(worker1)
    .worker(worker2)
    .metrics(MetricsSettings::default())
    .build()?;
```

### Pause and Resume

```rust
// Pause a specific worker
handler.pause_worker("my_task");

// Resume it
handler.resume_worker("my_task");

// Pause all workers
handler.pause_all();

// Resume all
handler.resume_all();
```

### Access Clients

The `TaskHandler` provides access to Conductor clients:

```rust
// Get the full client
let conductor_client = handler.conductor_client();

// Get specific clients
let workflow_client = conductor_client.workflow_client();
let metadata_client = handler.metadata_client();
let task_client = handler.task_client();
```

---

## Worker Configuration

### Programmatic Configuration

```rust
let worker = FnWorker::new("my_task", handler_fn)
    .with_thread_count(10)           // Max concurrent executions
    .with_poll_interval_millis(100)  // Poll every 100ms
    .with_domain("my_domain")        // Task routing domain
    .with_identity("worker-1");      // Worker identifier
```

### Environment Variables

Workers can be configured via environment variables:

```bash
# Global defaults
export CONDUCTOR_WORKER_POLL_INTERVAL=100
export CONDUCTOR_WORKER_DOMAIN=default

# Task-specific configuration
export CONDUCTOR_WORKER_MY_TASK_POLL_INTERVAL=50
export CONDUCTOR_WORKER_MY_TASK_THREAD_COUNT=20
export CONDUCTOR_WORKER_MY_TASK_DOMAIN=production
```

### Configuration Priority

1. Environment variables (highest)
2. Programmatic configuration
3. Default values (lowest)

---

## WorkerOutput Types

### Completed

Task completed successfully:

```rust
// With a single result value
Ok(WorkerOutput::completed_with_result("success"))

// With a map of outputs
let mut output = HashMap::new();
output.insert("status".to_string(), json!("processed"));
output.insert("count".to_string(), json!(42));
Ok(WorkerOutput::completed(output))

// With no output data
Ok(WorkerOutput::complete())
```

### Failed

Task failed (will retry based on task definition):

```rust
Ok(WorkerOutput::failed("Database connection error"))
```

### In Progress

Task is still running (for long-running tasks):

```rust
// Call back in 60 seconds
Ok(WorkerOutput::in_progress(60))
```

---

## Error Handling

### Returning Errors vs Failed Output

```rust
// Using WorkerOutput::failed - task fails, may retry
let worker = FnWorker::new("my_task", |task| async move {
    if let Err(e) = validate_input(&task) {
        return Ok(WorkerOutput::failed(format!("Validation error: {}", e)));
    }
    // ... process ...
    Ok(WorkerOutput::complete())
});

// Using Err - task fails with error, may retry
let worker = FnWorker::new("my_task", |task| async move {
    let data = fetch_data().await
        .map_err(|e| ConductorError::worker(format!("Fetch failed: {}", e)))?;
    Ok(WorkerOutput::completed_with_result(data))
});
```

### Retryable vs Terminal Errors

Configure retry behavior in the task definition:

```rust
use conductor::models::{TaskDef, RetryLogic, TimeoutPolicy};

let task_def = TaskDef::new("my_task")
    .with_retry(3, RetryLogic::ExponentialBackoff, 60)  // 3 retries, exponential backoff, 60s base delay
    .with_timeout(300, TimeoutPolicy::Retry);           // 5 min timeout, retry on timeout
```

---

## Long-Running Tasks

For tasks that take longer than the response timeout:

### Using In Progress

```rust
use conductor::models::TaskInProgress;

let worker = FnWorker::new("long_task", |task: Task| async move {
    let progress: i32 = task.get_input("_progress").unwrap_or(0);
    
    if progress < 100 {
        // Do some work
        let new_progress = progress + 10;
        
        // Return in-progress with callback
        let in_progress = TaskInProgress::new(30)  // Call back in 30 seconds
            .with_output("progress", new_progress);
        
        return Ok(WorkerOutput::InProgress(in_progress));
    }
    
    // Completed
    Ok(WorkerOutput::completed_with_result("Done!"))
});
```

---

## Accessing Task Data

### Input Data

```rust
let worker = FnWorker::new("my_task", |task: Task| async move {
    // Get typed input with default
    let count: i32 = task.get_input("count").unwrap_or(0);
    let name: String = task.get_input("name").unwrap_or_else(|| "default".to_string());
    
    // Get optional input
    let optional: Option<String> = task.get_input("optional");
    
    // Get raw JSON value
    if let Some(value) = task.input_data.get("complex") {
        // Handle complex value
    }
    
    Ok(WorkerOutput::complete())
});
```

### Task Metadata

Access task metadata directly or via the `TaskContext`:

```rust
let worker = FnWorker::new("my_task", |task: Task| async move {
    // Direct access to common fields
    let task_id = task.task_id();
    let workflow_id = task.workflow_instance_id();
    let poll_count = task.poll_count();
    let retry_count = task.retry_count();
    
    // Check execution state
    if task.is_first_poll() {
        println!("First time processing this task");
    }
    
    if task.is_retry() {
        println!("This is retry #{}", retry_count);
    }
    
    Ok(WorkerOutput::complete())
});
```

### TaskContext

For more detailed task context, use `task.context()`:

```rust
use conductor::worker::TaskContext;

let worker = FnWorker::new("my_task", |task: Task| async move {
    let ctx = task.context();
    
    // All task metadata in one place
    println!("Task ID: {}", ctx.task_id());
    println!("Workflow ID: {}", ctx.workflow_instance_id());
    println!("Task Type: {}", ctx.task_type());
    println!("Reference Name: {}", ctx.reference_task_name());
    
    // Execution state
    println!("Poll Count: {}", ctx.poll_count());
    println!("Retry Count: {}", ctx.retry_count());
    println!("Iteration: {}", ctx.iteration());
    
    // Timing (epoch milliseconds)
    println!("Scheduled: {}", ctx.scheduled_time());
    println!("Started: {}", ctx.start_time());
    
    // Optional fields
    if let Some(correlation_id) = ctx.correlation_id() {
        println!("Correlation ID: {}", correlation_id);
    }
    if let Some(domain) = ctx.domain() {
        println!("Domain: {}", domain);
    }
    
    // State checks
    if ctx.is_first_poll() {
        // Initialize resources for first poll
    }
    
    if ctx.is_retry() {
        // Handle retry-specific logic
    }
    
    Ok(WorkerOutput::complete())
});
```

### Long-Running Task with Poll Count

Use poll count to track progress in long-running tasks:

```rust
let worker = FnWorker::new("batch_processor", |task: Task| async move {
    let ctx = task.context();
    let batch_size = 100;
    let offset = ctx.poll_count() * batch_size;
    
    // Process a batch of items
    let items = fetch_items(offset, batch_size).await?;
    
    if items.len() < batch_size {
        // All done
        Ok(WorkerOutput::completed_with_result("Processing complete"))
    } else {
        // More to process - callback in 5 seconds
        Ok(WorkerOutput::in_progress(5))
    }
});
```

---

## Metrics and Events

### Enable Metrics

```rust
use conductor::metrics::MetricsSettings;

let mut handler = TaskHandler::new(config)?;
handler.enable_metrics(
    MetricsSettings::new()
        .with_http_port(9090)
        .with_metrics_path("/metrics"),
);
```

Metrics are available at `http://localhost:9090/metrics`. The SDK emits the
full canonical Prometheus catalog (counters, histograms, and gauges) covering
worker polling, task execution, task result updates, HTTP API client latency,
and more.

See [METRICS.md](../METRICS.md) for the complete metric catalog, label
definitions, bucket sets, and configuration details.

### Event Listeners

```rust
use conductor::events::{TaskRunnerEventsListener, TaskEvent};
use std::sync::Arc;

struct MyListener;

impl TaskRunnerEventsListener for MyListener {
    fn on_task_started(&self, event: &TaskEvent) {
        println!("Task started: {}", event.task_id);
    }
    
    fn on_task_completed(&self, event: &TaskEvent) {
        println!("Task completed: {}", event.task_id);
    }
    
    fn on_task_failed(&self, event: &TaskEvent) {
        println!("Task failed: {} - {:?}", event.task_id, event.error);
    }
}

handler.add_event_listener(Arc::new(MyListener));
```

---

## JSON Schema Support

Define input/output schemas for validation:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct MyInput {
    name: String,
    count: i32,
    #[serde(default)]
    optional: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct MyOutput {
    result: String,
    processed_count: i32,
}

let worker = FnWorker::new("schema_task", |task: Task| async move {
    let input: MyInput = task.get_input("input").unwrap_or_default();
    
    Ok(WorkerOutput::completed_with_result(MyOutput {
        result: format!("Hello, {}!", input.name),
        processed_count: input.count,
    }))
})
.with_input_schema_from::<MyInput>(true)   // strict mode
.with_output_schema_from::<MyOutput>(true);
```

---

## Best Practices

### 1. Idempotent Workers

Design workers to be idempotent - safe to execute multiple times:

```rust
let worker = FnWorker::new("process_order", |task: Task| async move {
    let order_id: String = task.get_input("order_id").unwrap_or_default();
    
    // Check if already processed
    if is_order_processed(&order_id).await? {
        return Ok(WorkerOutput::completed_with_result("Already processed"));
    }
    
    // Process and mark as done atomically
    process_and_mark_complete(&order_id).await?;
    
    Ok(WorkerOutput::complete())
});
```

### 2. Appropriate Thread Count

```rust
// I/O-bound tasks: higher thread count
let io_worker = FnWorker::new("api_call", handler)
    .with_thread_count(50);

// CPU-bound tasks: match CPU cores
let cpu_worker = FnWorker::new("compute", handler)
    .with_thread_count(num_cpus::get());

// External rate-limited APIs: limit concurrency
let rate_limited_worker = FnWorker::new("external_api", handler)
    .with_thread_count(5);
```

### 3. Graceful Error Handling

```rust
let worker = FnWorker::new("robust_task", |task: Task| async move {
    // Validate inputs
    let required: String = task.get_input("required")
        .ok_or_else(|| ConductorError::worker("Missing required input"))?;
    
    // Handle transient errors with retry
    let result = retry_with_backoff(|| async {
        external_api_call(&required).await
    }).await?;
    
    Ok(WorkerOutput::completed_with_result(result))
});
```

### 4. Structured Logging

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(task), fields(task_id = %task.task_id, workflow_id = %task.workflow_instance_id))]
async fn process_task(task: Task) -> Result<WorkerOutput> {
    info!("Processing task");
    
    match do_work(&task).await {
        Ok(result) => {
            info!(result = ?result, "Task completed successfully");
            Ok(WorkerOutput::completed_with_result(result))
        }
        Err(e) => {
            error!(error = %e, "Task failed");
            Ok(WorkerOutput::failed(e.to_string()))
        }
    }
}
```

### 5. Resource Management

```rust
// Share expensive resources across executions
struct DatabaseWorker {
    pool: Arc<DatabasePool>,
}

#[async_trait]
impl Worker for DatabaseWorker {
    fn task_definition_name(&self) -> &str {
        "db_task"
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        // Reuse connection from pool
        let conn = self.pool.get().await?;
        let result = conn.query(&task.get_input::<String>("query").unwrap_or_default()).await?;
        Ok(WorkerOutput::completed_with_result(result))
    }
}
```

---

## See Also

- [Task Management](./TASK_MANAGEMENT.md) - Low-level task APIs
- [Workflow Management](./WORKFLOW.md) - Running workflows
- [Metadata Management](./METADATA.md) - Registering task definitions
