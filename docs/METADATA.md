# Metadata Management API Reference

Complete API reference for metadata management operations in the Conductor Rust SDK. This covers workflow definitions, task definitions, and their associated tags.

## Table of Contents

- [Quick Start](#quick-start)
- [MetadataClient API](#metadataclient-api)
- [Workflow Definitions](#workflow-definitions)
- [Task Definitions](#task-definitions)
- [Tagging](#tagging)
- [Models Reference](#models-reference)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::{WorkflowDef, TaskDef, WorkflowTask, MetadataTag};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("http://localhost:8080/api");
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();

    // Register a task definition
    let task_def = TaskDef::new("process_order")
        .with_description("Process incoming orders")
        .with_retry(3, RetryLogic::ExponentialBackoff, 60)
        .with_timeout(300, TimeoutPolicy::TimeOutWf);
    
    metadata_client.register_task_def(&task_def).await?;

    // Register a workflow definition
    let workflow = WorkflowDef::new("order_workflow")
        .with_description("Order processing workflow")
        .with_task(WorkflowTask::simple("process_order", "process_ref"));
    
    metadata_client.register_workflow_def(&workflow).await?;

    println!("Definitions registered successfully!");
    Ok(())
}
```

---

## MetadataClient API

### Workflow Definition APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `register_workflow_def()` | `POST /metadata/workflow` | Register new workflow |
| `update_workflow_def()` | `PUT /metadata/workflow` | Update existing workflow |
| `register_or_update_workflow_def()` | POST/PUT | Register or update workflow |
| `get_workflow_def()` | `GET /metadata/workflow/{name}` | Get workflow by name |
| `get_all_workflow_def_versions()` | `GET /metadata/workflow/{name}/versions` | Get all versions |
| `get_all_workflow_defs()` | `GET /metadata/workflow` | Get all workflows |
| `delete_workflow_def()` | `DELETE /metadata/workflow/{name}/{version}` | Delete workflow |
| `workflow_def_exists()` | `GET /metadata/workflow/{name}` | Check if exists |

### Task Definition APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `register_task_def()` | `POST /metadata/taskdefs` | Register new task |
| `register_task_defs()` | `POST /metadata/taskdefs` | Register multiple tasks |
| `update_task_def()` | `PUT /metadata/taskdefs` | Update existing task |
| `get_task_def()` | `GET /metadata/taskdefs/{name}` | Get task by name |
| `get_all_task_defs()` | `GET /metadata/taskdefs` | Get all tasks |
| `delete_task_def()` | `DELETE /metadata/taskdefs/{name}` | Delete task |
| `task_def_exists()` | `GET /metadata/taskdefs/{name}` | Check if exists |

### Tag APIs

| Method | Endpoint | Description |
|--------|----------|-------------|
| `add_workflow_tag()` | `POST /metadata/workflow/{name}/tags` | Add workflow tag |
| `get_workflow_tags()` | `GET /metadata/workflow/{name}/tags` | Get workflow tags |
| `set_workflow_tags()` | `PUT /metadata/workflow/{name}/tags` | Replace workflow tags |
| `delete_workflow_tag()` | `DELETE /metadata/workflow/{name}/tags` | Delete workflow tag |
| `add_task_tag()` | `POST /metadata/taskdefs/{name}/tags` | Add task tag |
| `get_task_tags()` | `GET /metadata/taskdefs/{name}/tags` | Get task tags |
| `set_task_tags()` | `PUT /metadata/taskdefs/{name}/tags` | Replace task tags |
| `delete_task_tag()` | `DELETE /metadata/taskdefs/{name}/tags` | Delete task tag |

---

## Workflow Definitions

### Register Workflow Definition

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("order_workflow")
    .with_description("Process customer orders")
    .with_version(1)
    .with_task(WorkflowTask::simple("validate_order", "validate_ref"))
    .with_task(WorkflowTask::simple("process_payment", "payment_ref"))
    .with_task(WorkflowTask::simple("ship_order", "ship_ref"));

metadata_client.register_workflow_def(&workflow).await?;
```

### Update Workflow Definition

```rust
let mut workflow = metadata_client.get_workflow_def("order_workflow", None).await?;

// Add a new task
workflow.tasks.push(WorkflowTask::simple("send_notification", "notify_ref"));

metadata_client.update_workflow_def(&workflow).await?;
```

### Register or Update (Upsert)

```rust
let workflow = WorkflowDef::new("my_workflow")
    .with_task(WorkflowTask::simple("task1", "task1_ref"));

// Will create if not exists, update if exists
metadata_client.register_or_update_workflow_def(&workflow, true).await?;
```

### Get Workflow Definition

```rust
// Get latest version
let workflow = metadata_client.get_workflow_def("order_workflow", None).await?;

// Get specific version
let workflow = metadata_client.get_workflow_def("order_workflow", Some(2)).await?;

println!("Workflow: {} v{}", workflow.name, workflow.version);
println!("Tasks: {}", workflow.tasks.len());
```

### Get All Versions

```rust
let versions = metadata_client.get_all_workflow_def_versions("order_workflow").await?;

for wf in versions {
    println!("Version {}: {} tasks", wf.version, wf.tasks.len());
}
```

### Get All Workflow Definitions

```rust
let workflows = metadata_client.get_all_workflow_defs().await?;

for wf in workflows {
    println!("{} v{}: {}", wf.name, wf.version, wf.description.unwrap_or_default());
}
```

### Delete Workflow Definition

```rust
metadata_client.delete_workflow_def("order_workflow", 1).await?;
```

### Check if Workflow Exists

```rust
if metadata_client.workflow_def_exists("order_workflow", None).await? {
    println!("Workflow exists");
} else {
    println!("Workflow not found");
}
```

---

## Task Definitions

### Register Task Definition

```rust
use conductor::models::{TaskDef, RetryLogic, TimeoutPolicy};

let task = TaskDef::new("process_payment")
    .with_description("Process payment transactions")
    .with_retry(3, RetryLogic::ExponentialBackoff, 60)
    .with_timeout(300, TimeoutPolicy::TimeOutWf)
    .with_response_timeout(120)
    .with_concurrent_limit(10)
    .with_rate_limit(100, 60);  // 100 per 60 seconds

metadata_client.register_task_def(&task).await?;
```

### Register Multiple Task Definitions

```rust
let tasks = vec![
    TaskDef::new("validate_order")
        .with_retry(2, RetryLogic::Fixed, 30),
    TaskDef::new("process_payment")
        .with_retry(3, RetryLogic::ExponentialBackoff, 60),
    TaskDef::new("send_notification")
        .with_retry(5, RetryLogic::Fixed, 10),
];

metadata_client.register_task_defs(&tasks).await?;
```

### Update Task Definition

```rust
let mut task = metadata_client.get_task_def("process_payment").await?;

// Update retry configuration
task.retry_count = 5;
task.retry_delay_seconds = 30;

metadata_client.update_task_def(&task).await?;
```

### Get Task Definition

```rust
let task = metadata_client.get_task_def("process_payment").await?;

println!("Task: {}", task.name);
println!("Retries: {}", task.retry_count);
println!("Timeout: {}s", task.timeout_seconds);
```

### Get All Task Definitions

```rust
let tasks = metadata_client.get_all_task_defs().await?;

for task in tasks {
    println!("{}: {} retries, {}s timeout", 
        task.name, 
        task.retry_count,
        task.timeout_seconds);
}
```

### Delete Task Definition

```rust
metadata_client.delete_task_def("process_payment").await?;
```

### Check if Task Exists

```rust
if metadata_client.task_def_exists("process_payment").await? {
    println!("Task exists");
} else {
    println!("Task not found, registering...");
    metadata_client.register_task_def(&task_def).await?;
}
```

---

## Tagging

### Workflow Tags

```rust
use conductor::models::MetadataTag;

// Add a single tag
let tag = MetadataTag::new("environment", "production");
metadata_client.add_workflow_tag("order_workflow", &tag).await?;

// Get all tags
let tags = metadata_client.get_workflow_tags("order_workflow").await?;
for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}

// Replace all tags
let new_tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("team", "orders"),
    MetadataTag::new("version", "2.0"),
];
metadata_client.set_workflow_tags("order_workflow", &new_tags).await?;

// Delete a tag
let tag_to_delete = MetadataTag::new("version", "2.0");
metadata_client.delete_workflow_tag("order_workflow", &tag_to_delete).await?;
```

### Task Tags

```rust
// Add a single tag
let tag = MetadataTag::new("type", "payment");
metadata_client.add_task_tag("process_payment", &tag).await?;

// Get all tags
let tags = metadata_client.get_task_tags("process_payment").await?;

// Replace all tags
let new_tags = vec![
    MetadataTag::new("type", "payment"),
    MetadataTag::new("integration", "stripe"),
    MetadataTag::new("async", "true"),
];
metadata_client.set_task_tags("process_payment", &new_tags).await?;

// Delete a tag
let tag = MetadataTag::new("async", "true");
metadata_client.delete_task_tag("process_payment", &tag).await?;
```

---

## Models Reference

### WorkflowDef

```rust
use conductor::models::WorkflowDef;

let workflow = WorkflowDef::new("my_workflow")
    .with_description("Description")
    .with_version(1)
    .with_input_parameters(vec!["param1".to_string(), "param2".to_string()])
    .with_output_parameters([
        ("result".to_string(), json!("${task_ref.output.result}"))
    ].into_iter().collect())
    .with_timeout_seconds(3600)
    .with_timeout_policy(WorkflowTimeoutPolicy::TimeOutWf)
    .with_failure_workflow("failure_handler")
    .with_restartable(true)
    .with_task(WorkflowTask::simple("task1", "task1_ref"));
```

**Key Fields:**
- `name`: Unique workflow name
- `version`: Version number (default: 1)
- `description`: Optional description
- `tasks`: List of workflow tasks
- `input_parameters`: Expected input parameters
- `output_parameters`: Output mapping
- `timeout_seconds`: Workflow timeout
- `failure_workflow`: Workflow to run on failure

### WorkflowTask

```rust
use conductor::models::WorkflowTask;

// Simple task
let task = WorkflowTask::simple("task_def_name", "reference_name")
    .with_input_param("key", json!("${workflow.input.value}"))
    .with_optional(true);

// HTTP task
let http_task = WorkflowTask::http("http_ref")
    .with_input_param("http_request", json!({
        "uri": "https://api.example.com/data",
        "method": "GET"
    }));

// Sub-workflow
let sub_workflow = WorkflowTask::sub_workflow("sub_wf_ref", "child_workflow")
    .with_version(1);

// Decision (switch)
let decision = WorkflowTask::switch("decision_ref", "${workflow.input.type}")
    .with_case("type_a", vec![WorkflowTask::simple("task_a", "task_a_ref")])
    .with_case("type_b", vec![WorkflowTask::simple("task_b", "task_b_ref")])
    .with_default(vec![WorkflowTask::simple("default", "default_ref")]);

// Fork/Join
let fork = WorkflowTask::fork_join(
    "fork_ref",
    vec![
        vec![WorkflowTask::simple("branch1", "b1_ref")],
        vec![WorkflowTask::simple("branch2", "b2_ref")],
    ],
);
```

### TaskDef

```rust
use conductor::models::{TaskDef, RetryLogic, TimeoutPolicy};

let task = TaskDef::new("my_task")
    .with_description("Task description")
    .with_retry(3, RetryLogic::ExponentialBackoff, 60)
    .with_timeout(300, TimeoutPolicy::Retry)
    .with_response_timeout(120)
    .with_concurrent_limit(10)
    .with_rate_limit(100, 60);
```

**Key Fields:**
- `name`: Unique task name
- `description`: Optional description
- `retry_count`: Number of retry attempts
- `retry_logic`: Fixed, ExponentialBackoff, LinearBackoff
- `retry_delay_seconds`: Delay between retries
- `timeout_seconds`: Task execution timeout
- `response_timeout_seconds`: Response timeout
- `timeout_policy`: Retry, TimeOutWf, AlertOnly
- `concurrent_exec_limit`: Max concurrent executions
- `rate_limit_per_frequency`: Rate limit count
- `rate_limit_frequency_in_seconds`: Rate limit window

### RetryLogic

```rust
pub enum RetryLogic {
    Fixed,              // Fixed delay between retries
    ExponentialBackoff, // Exponential backoff
    LinearBackoff,      // Linear backoff
}
```

### TimeoutPolicy

```rust
pub enum TimeoutPolicy {
    Retry,     // Retry the task on timeout
    TimeOutWf, // Timeout the workflow
    AlertOnly, // Just alert, don't take action
}
```

### MetadataTag

```rust
use conductor::models::MetadataTag;

let tag = MetadataTag::new("key", "value");

// Or
let tag = MetadataTag {
    key: "environment".to_string(),
    value: "production".to_string(),
};
```

---

## Best Practices

### 1. Version Your Workflows

```rust
// Always specify version explicitly
let workflow = WorkflowDef::new("order_workflow")
    .with_version(2)  // Increment for breaking changes
    .with_task(/* ... */);

// Keep old versions for running workflows
// Delete old versions only when no workflows use them
```

### 2. Configure Appropriate Retry Policies

```rust
// For transient failures (network, timeouts)
let task = TaskDef::new("api_call")
    .with_retry(5, RetryLogic::ExponentialBackoff, 30);

// For quick retries (race conditions)
let task = TaskDef::new("cache_update")
    .with_retry(3, RetryLogic::Fixed, 1);

// For no-retry scenarios
let task = TaskDef::new("idempotent_operation")
    .with_retry(0, RetryLogic::Fixed, 0);
```

### 3. Set Appropriate Timeouts

```rust
// Quick tasks
let task = TaskDef::new("quick_validation")
    .with_timeout(60, TimeoutPolicy::Retry)
    .with_response_timeout(30);

// Long-running tasks
let task = TaskDef::new("data_processing")
    .with_timeout(3600, TimeoutPolicy::TimeOutWf)
    .with_response_timeout(600);
```

### 4. Use Tags for Organization

```rust
// Tag by environment
let env_tag = MetadataTag::new("environment", "production");

// Tag by team
let team_tag = MetadataTag::new("team", "payments");

// Tag by version
let version_tag = MetadataTag::new("api_version", "v2");

metadata_client.set_workflow_tags("payment_workflow", &[
    env_tag, team_tag, version_tag
]).await?;
```

### 5. Use Rate Limits

```rust
// Protect external APIs
let task = TaskDef::new("external_api_call")
    .with_rate_limit(100, 60)      // 100 per minute
    .with_concurrent_limit(10);    // Max 10 concurrent

// Heavy processing tasks
let task = TaskDef::new("heavy_computation")
    .with_concurrent_limit(4);     // Limit by CPU cores
```

### 6. Validate Before Registration

```rust
async fn register_safe(
    client: &MetadataClient,
    task: &TaskDef,
) -> Result<()> {
    // Validate task name
    if task.name.is_empty() {
        return Err(ConductorError::validation("Task name cannot be empty"));
    }

    // Check timeout constraints
    if task.response_timeout_seconds >= task.timeout_seconds {
        return Err(ConductorError::validation(
            "Response timeout must be less than task timeout"
        ));
    }

    // Register
    client.register_task_def(task).await
}
```

---

## See Also

- [Workflow Management](./WORKFLOW.md) - Running workflows
- [Worker Implementation](./WORKER.md) - Implementing workers
- [Task Management](./TASK_MANAGEMENT.md) - Task operations
