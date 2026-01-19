# Task Management API Reference

Complete API reference for task management operations in the Conductor Rust SDK. This covers task polling, updating, logging, and queue management.

## Table of Contents

- [Quick Start](#quick-start)
- [TaskClient API](#taskclient-api)
- [Polling Tasks](#polling-tasks)
- [Updating Tasks](#updating-tasks)
- [Task Logging](#task-logging)
- [Queue Management](#queue-management)
- [Models Reference](#models-reference)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::{Task, TaskResult, TaskResultStatus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("http://localhost:8080/api");
    let client = ConductorClient::new(config)?;
    let task_client = client.task_client();

    // Poll for a task
    if let Some(task) = task_client.poll_task("my_task", Some("worker-1"), None).await? {
        println!("Got task: {}", task.task_id);

        // Process the task...
        let result = TaskResult::completed(&task.task_id, &task.workflow_instance_id)
            .with_output_value("result", "success");

        // Update the task
        task_client.update_task(&result).await?;
    }

    Ok(())
}
```

---

## TaskClient API

### Polling APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `poll_task()` | `GET /tasks/poll/{taskType}` | Poll for a single task |
| `batch_poll()` | `GET /tasks/poll/batch/{taskType}` | Poll for multiple tasks |

### Task Management APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `get_task()` | `GET /tasks/{taskId}` | Get task by ID |
| `update_task()` | `POST /tasks` | Update task with result |
| `update_task_with_retry()` | `POST /tasks` | Update with retry logic |
| `update_task_by_ref_name()` | `POST /tasks/{wfId}/{taskRef}/{status}` | Update by reference name |
| `update_task_sync()` | `POST /tasks/{wfId}/{taskRef}/{status}/sync` | Update and get workflow |

### Logging APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `add_task_log()` | `POST /tasks/{taskId}/log` | Add log message to task |
| `get_task_logs()` | `GET /tasks/{taskId}/log` | Get all task logs |

### Queue Management APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `get_queue_sizes()` | `GET /tasks/queue/sizes` | Get queue sizes for task types |
| `get_queue_size_for_task()` | `GET /tasks/queue/sizes` | Get queue size for one task type |
| `get_task_poll_data()` | `GET /tasks/queue/polldata/{taskType}` | Get poll data for task type |
| `get_all_poll_data()` | `GET /tasks/queue/polldata/all` | Get all poll data |
| `get_tasks_in_progress()` | `GET /tasks/in_progress/{taskType}` | Get in-progress tasks |
| `remove_task_from_queue()` | `DELETE /tasks/queue/{taskType}/{taskId}` | Remove task from queue |

---

## Polling Tasks

### Poll Single Task

```rust
// Basic poll
let task = task_client.poll_task("my_task", None, None).await?;

// With worker ID
let task = task_client.poll_task(
    "my_task",
    Some("worker-1"),
    None,
).await?;

// With domain routing
let task = task_client.poll_task(
    "my_task",
    Some("worker-1"),
    Some("production"),
).await?;

if let Some(task) = task {
    println!("Task ID: {}", task.task_id);
    println!("Workflow ID: {}", task.workflow_instance_id);
    println!("Input: {:?}", task.input_data);
}
```

### Batch Poll

Poll multiple tasks at once for higher throughput:

```rust
use std::time::Duration;

// Poll up to 10 tasks with 100ms timeout
let tasks = task_client.batch_poll(
    "my_task",
    Some("worker-1"),
    None,                          // domain
    10,                            // count
    Duration::from_millis(100),    // timeout
).await?;

println!("Received {} tasks", tasks.len());

for task in tasks {
    // Process each task
}
```

---

## Updating Tasks

### Basic Update

```rust
use conductor::models::{TaskResult, TaskResultStatus};

// Create task result
let result = TaskResult::completed(&task.task_id, &task.workflow_instance_id)
    .with_worker_id("worker-1")
    .with_output_value("result", "processed")
    .with_output_value("count", 42);

// Update the task
task_client.update_task(&result).await?;
```

### Update with Different Statuses

```rust
// Completed
let result = TaskResult::completed(&task.task_id, &task.workflow_instance_id)
    .with_output_value("success", true);

// Failed (will retry based on task definition)
let result = TaskResult::failed(&task.task_id, &task.workflow_instance_id)
    .with_reason("Database connection failed");

// Failed with terminal error (no retry)
let result = TaskResult {
    task_id: task.task_id.clone(),
    workflow_instance_id: task.workflow_instance_id.clone(),
    status: TaskResultStatus::FailedWithTerminalError,
    reason_for_incompletion: Some("Invalid input - cannot process".to_string()),
    ..Default::default()
};

// In Progress (for long-running tasks)
let result = TaskResult::in_progress(&task.task_id, &task.workflow_instance_id)
    .with_callback_after_seconds(30)
    .with_output_value("progress", 50);
```

### Update with Retry

Automatically retry on transient failures:

```rust
let result = TaskResult::completed(&task.task_id, &task.workflow_instance_id)
    .with_output_value("result", "done");

// Will retry up to 3 times with exponential backoff
task_client.update_task_with_retry(&result, 3).await?;
```

### Update by Reference Name

Update a task using workflow ID and task reference name:

```rust
use serde_json::json;

let response = task_client.update_task_by_ref_name(
    &workflow_id,
    "task_reference_name",
    TaskResultStatus::Completed,
    json!({ "result": "success" }),
    Some("worker-1"),
).await?;
```

### Update Synchronously

Update task and get the resulting workflow state:

```rust
let workflow = task_client.update_task_sync(
    &workflow_id,
    "task_reference_name",
    TaskResultStatus::Completed,
    json!({ "result": "done" }),
    Some("worker-1"),
).await?;

println!("Workflow status: {:?}", workflow.status);
```

---

## Task Logging

### Add Log to Task

```rust
// Add a single log entry
task_client.add_task_log(&task.task_id, "Starting processing").await?;

// Log progress
for i in 0..10 {
    task_client.add_task_log(
        &task.task_id,
        &format!("Processing batch {}/10", i + 1),
    ).await?;
    
    // Process batch...
}

// Log completion
task_client.add_task_log(&task.task_id, "Processing complete").await?;
```

### Get Task Logs

```rust
let logs = task_client.get_task_logs(&task.task_id).await?;

for log in logs {
    println!("[{}] {}", log.created_time, log.log);
}
```

---

## Queue Management

### Get Queue Sizes

```rust
// Get sizes for multiple task types
let sizes = task_client.get_queue_sizes(&["task1", "task2", "task3"]).await?;

for (task_type, size) in sizes {
    println!("{}: {} tasks in queue", task_type, size);
}

// Get size for a single task type
let size = task_client.get_queue_size_for_task("my_task").await?;
println!("Queue depth: {}", size);
```

### Get Poll Data

Monitor worker polling activity:

```rust
// Get poll data for a specific task type
let poll_data = task_client.get_task_poll_data("my_task").await?;

for data in poll_data {
    println!("Queue: {}", data.queue_name);
    if let Some(worker) = &data.worker_id {
        println!("  Last worker: {}", worker);
    }
    println!("  Last poll: {}", data.last_poll_time);
}

// Get all poll data
let all_data = task_client.get_all_poll_data().await?;
```

### Get Tasks In Progress

```rust
let tasks = task_client.get_tasks_in_progress(
    "my_task",
    None,        // start_key for pagination
    Some(100),   // count
).await?;

println!("Tasks in progress: {}", tasks.len());
for task in tasks {
    println!("  {} - started at {}", task.task_id, task.start_time);
}
```

### Remove Task from Queue

Manually remove a task from the queue:

```rust
task_client.remove_task_from_queue("my_task", &task_id).await?;
```

---

## Models Reference

### Task

The task object returned from polling:

```rust
pub struct Task {
    pub task_id: String,
    pub workflow_instance_id: String,
    pub task_def_name: String,
    pub reference_task_name: String,
    pub workflow_type: String,
    pub status: TaskStatus,
    pub input_data: HashMap<String, Value>,
    pub output_data: HashMap<String, Value>,
    pub scheduled_time: i64,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub retry_count: i32,
    pub poll_count: i32,
    pub correlation_id: Option<String>,
    pub reason_for_incompletion: Option<String>,
    // ... additional fields
}

impl Task {
    // Get typed input value
    pub fn get_input<T: DeserializeOwned>(&self, key: &str) -> Option<T>;
    
    // Get input as string
    pub fn get_input_string(&self, key: &str) -> Option<String>;
    
    // Get input as i32
    pub fn get_input_i32(&self, key: &str) -> Option<i32>;
}
```

### TaskResult

Result for updating a task:

```rust
pub struct TaskResult {
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: Option<String>,
    pub status: TaskResultStatus,
    pub output_data: HashMap<String, Value>,
    pub reason_for_incompletion: Option<String>,
    pub callback_after_seconds: i64,
    pub logs: Vec<String>,
}

impl TaskResult {
    // Create completed result
    pub fn completed(task_id: &str, workflow_id: &str) -> Self;
    
    // Create failed result
    pub fn failed(task_id: &str, workflow_id: &str) -> Self;
    
    // Create in-progress result
    pub fn in_progress(task_id: &str, workflow_id: &str) -> Self;
    
    // Builder methods
    pub fn with_worker_id(self, id: impl Into<String>) -> Self;
    pub fn with_output_value(self, key: impl Into<String>, value: impl Serialize) -> Self;
    pub fn with_output(self, output: HashMap<String, Value>) -> Self;
    pub fn with_reason(self, reason: impl Into<String>) -> Self;
    pub fn with_callback_after_seconds(self, seconds: i64) -> Self;
    pub fn with_log(self, log: impl Into<String>) -> Self;
}
```

### TaskResultStatus

```rust
pub enum TaskResultStatus {
    Completed,              // Task completed successfully
    Failed,                 // Task failed (may retry)
    FailedWithTerminalError, // Task failed (no retry)
    InProgress,             // Task still running
}
```

### TaskExecLog

```rust
pub struct TaskExecLog {
    pub log: String,
    pub task_id: String,
    pub created_time: i64,
}
```

### PollData

```rust
pub struct PollData {
    pub queue_name: String,
    pub domain: Option<String>,
    pub worker_id: Option<String>,
    pub last_poll_time: i64,
}
```

---

## Best Practices

### 1. Use Batch Polling for Throughput

```rust
// Instead of polling one at a time
loop {
    if let Some(task) = task_client.poll_task("task", Some("worker"), None).await? {
        process(task).await;
    }
}

// Use batch polling
loop {
    let tasks = task_client.batch_poll("task", Some("worker"), None, 10, Duration::from_millis(100)).await?;
    for task in tasks {
        tokio::spawn(process(task));
    }
}
```

### 2. Handle Update Failures

```rust
async fn safe_update(client: &TaskClient, result: &TaskResult) -> Result<()> {
    match client.update_task_with_retry(result, 3).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Log the failure for debugging
            eprintln!("Failed to update task {}: {}", result.task_id, e);
            
            // Task will timeout and be retried by Conductor
            Err(e)
        }
    }
}
```

### 3. Use Logging for Debugging

```rust
async fn process_with_logging(client: &TaskClient, task: Task) -> Result<TaskResult> {
    client.add_task_log(&task.task_id, "Starting task processing").await.ok();
    
    // Validate input
    let input = match task.get_input::<MyInput>("data") {
        Some(data) => data,
        None => {
            client.add_task_log(&task.task_id, "ERROR: Missing required input 'data'").await.ok();
            return Ok(TaskResult::failed(&task.task_id, &task.workflow_instance_id)
                .with_reason("Missing required input"));
        }
    };
    
    client.add_task_log(&task.task_id, &format!("Processing: {:?}", input)).await.ok();
    
    // Process...
    
    client.add_task_log(&task.task_id, "Task completed successfully").await.ok();
    
    Ok(TaskResult::completed(&task.task_id, &task.workflow_instance_id))
}
```

### 4. Monitor Queue Depth

```rust
async fn health_check(client: &TaskClient) -> bool {
    let size = client.get_queue_size_for_task("critical_task").await.unwrap_or(0);
    
    if size > 1000 {
        eprintln!("WARNING: Queue depth is high: {}", size);
        // Trigger alert or scale workers
    }
    
    size < 5000  // Return healthy if under threshold
}
```

### 5. Handle Long-Running Tasks

```rust
async fn long_running_task(client: &TaskClient, task: Task) -> Result<TaskResult> {
    let total_items: i32 = task.get_input("total_items").unwrap_or(0);
    let processed: i32 = task.get_input("_processed").unwrap_or(0);
    
    // Process a batch
    let batch_size = 100;
    let new_processed = processed + batch_size;
    
    if new_processed < total_items {
        // More work to do - report progress and callback
        Ok(TaskResult::in_progress(&task.task_id, &task.workflow_instance_id)
            .with_callback_after_seconds(30)
            .with_output_value("_processed", new_processed)
            .with_output_value("progress", (new_processed * 100) / total_items))
    } else {
        // Done
        Ok(TaskResult::completed(&task.task_id, &task.workflow_instance_id)
            .with_output_value("total_processed", new_processed))
    }
}
```

---

## See Also

- [Worker Implementation](./WORKER.md) - High-level worker abstraction
- [Workflow Management](./WORKFLOW.md) - Workflow operations
- [Metadata Management](./METADATA.md) - Task definitions
