# Workflow Management API Reference

Complete API reference for workflow management operations in the Conductor Rust SDK.

## Table of Contents

- [Quick Start](#quick-start)
- [WorkflowClient API](#workflowclient-api)
- [Starting Workflows](#starting-workflows)
- [Workflow Lifecycle](#workflow-lifecycle)
- [Searching Workflows](#searching-workflows)
- [Advanced Operations](#advanced-operations)
- [Models Reference](#models-reference)
- [Error Handling](#error-handling)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::StartWorkflowRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("http://localhost:8080/api");
    let client = ConductorClient::new(config)?;
    let workflow_client = client.workflow_client();

    // Start a workflow
    let request = StartWorkflowRequest::new("order_workflow")
        .with_version(1)
        .with_input_value("order_id", "ORD-123")
        .with_input_value("customer_id", "CUST-456");

    let workflow_id = workflow_client.start_workflow(&request).await?;
    println!("Started workflow: {}", workflow_id);

    // Get workflow status
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    println!("Status: {:?}", workflow.status);

    Ok(())
}
```

---

## WorkflowClient API

### API Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| `start_workflow()` | `POST /workflow` | Start workflow asynchronously |
| `execute_workflow()` | `POST /workflow/execute/{name}/{version}` | Start and wait for completion |
| `get_workflow()` | `GET /workflow/{id}` | Get workflow by ID |
| `get_workflow_status()` | `GET /workflow/{id}/status` | Get workflow status |
| `pause_workflow()` | `PUT /workflow/{id}/pause` | Pause a running workflow |
| `resume_workflow()` | `PUT /workflow/{id}/resume` | Resume a paused workflow |
| `terminate_workflow()` | `DELETE /workflow/{id}` | Terminate a workflow |
| `restart_workflow()` | `POST /workflow/{id}/restart` | Restart from beginning |
| `retry_workflow()` | `POST /workflow/{id}/retry` | Retry failed workflow |
| `rerun_workflow()` | `POST /workflow/{id}/rerun` | Rerun from specific task |
| `delete_workflow()` | `DELETE /workflow/{id}` | Delete workflow execution |
| `search_workflows()` | `GET /workflow/search` | Search workflows |
| `get_running_workflows()` | `GET /workflow/running/{name}` | Get running workflows by name |

---

## Starting Workflows

### Start Workflow Asynchronously

Start a workflow and return immediately with the workflow ID:

```rust
use conductor::models::StartWorkflowRequest;

let request = StartWorkflowRequest::new("my_workflow")
    .with_version(1)
    .with_input_value("param1", "value1")
    .with_input_value("param2", 42)
    .with_correlation_id("order-123")
    .with_task_to_domain("task_type", "production");

let workflow_id = workflow_client.start_workflow(&request).await?;
println!("Workflow started: {}", workflow_id);
```

### Execute Workflow Synchronously

Start a workflow and wait for completion:

```rust
use std::time::Duration;

let request = StartWorkflowRequest::new("quick_workflow")
    .with_input_value("data", "process this");

// Wait up to 30 seconds for completion
let workflow = workflow_client.execute_workflow(&request, Duration::from_secs(30)).await?;

println!("Workflow completed with status: {:?}", workflow.status);
println!("Output: {:?}", workflow.output);
```

### Execute with Return Strategy

Advanced execution with custom return options:

```rust
let request = StartWorkflowRequest::new("my_workflow")
    .with_input_value("data", "value");

let response = workflow_client.execute_workflow_with_return_strategy(
    &request,
    Some("request-123"),           // request_id
    Some("wait_task_ref"),         // wait_until_task_ref
    60,                            // wait_for_seconds
    Some("EVENTUAL"),              // consistency
    Some("WORKFLOW_STATUS"),       // return_strategy
).await?;

println!("Workflow ID: {}", response.workflow_id);
println!("Status: {:?}", response.status);
```

---

## Workflow Lifecycle

### Get Workflow

Retrieve workflow execution details:

```rust
// Without tasks
let workflow = workflow_client.get_workflow(&workflow_id, false).await?;

// With tasks
let workflow = workflow_client.get_workflow(&workflow_id, true).await?;

println!("Workflow: {}", workflow.workflow_id);
println!("Status: {:?}", workflow.status);
println!("Tasks: {}", workflow.tasks.len());
```

### Get Workflow Status

Get lightweight status information:

```rust
let workflow = workflow_client.get_workflow_status(
    &workflow_id,
    true,   // include_output
    true,   // include_variables
).await?;

println!("Status: {:?}", workflow.status);
println!("Output: {:?}", workflow.output);
println!("Variables: {:?}", workflow.variables);
```

### Pause Workflow

Pause a running workflow:

```rust
workflow_client.pause_workflow(&workflow_id).await?;
println!("Workflow paused");
```

### Resume Workflow

Resume a paused workflow:

```rust
workflow_client.resume_workflow(&workflow_id).await?;
println!("Workflow resumed");
```

### Terminate Workflow

Terminate a running workflow:

```rust
workflow_client.terminate_workflow(
    &workflow_id,
    Some("Manual termination"),  // reason
    false,                       // trigger_failure_workflow
).await?;
println!("Workflow terminated");
```

### Restart Workflow

Restart a workflow from the beginning:

```rust
// Use the same workflow definition version
workflow_client.restart_workflow(&workflow_id, false).await?;

// Use the latest workflow definition
workflow_client.restart_workflow(&workflow_id, true).await?;
```

### Retry Failed Workflow

Retry a failed workflow from the failed task:

```rust
workflow_client.retry_workflow(
    &workflow_id,
    true,  // resume_subworkflow_tasks
).await?;
```

### Rerun Workflow

Rerun from a specific task with new inputs:

```rust
use std::collections::HashMap;

let mut task_input = HashMap::new();
task_input.insert("new_param".to_string(), json!("new_value"));

let new_workflow_id = workflow_client.rerun_workflow(
    &workflow_id,
    "task_ref_to_rerun",
    Some(task_input),      // task_input
    None,                  // workflow_input
).await?;
```

### Delete Workflow

Delete a workflow execution:

```rust
// Delete without archiving
workflow_client.delete_workflow(&workflow_id, false).await?;

// Delete and archive
workflow_client.delete_workflow(&workflow_id, true).await?;
```

---

## Searching Workflows

### Basic Search

```rust
let result = workflow_client.search_workflows(
    Some("status=RUNNING"),  // query
    None,                    // free_text
    0,                       // start
    100,                     // size
).await?;

println!("Total hits: {}", result.total_hits);
for workflow in result.results {
    println!("  {} - {:?}", workflow.workflow_id, workflow.status);
}
```

### Search with Free Text

```rust
let result = workflow_client.search_workflows(
    None,
    Some("order-123"),  // free_text search
    0,
    50,
).await?;
```

### Get Running Workflows

```rust
let workflow_ids = workflow_client.get_running_workflows(
    "order_workflow",
    Some(1),                        // version
    Some(1704067200000),            // start_time (epoch ms)
    Some(1704153600000),            // end_time (epoch ms)
).await?;

println!("Running workflows: {:?}", workflow_ids);
```

### Get by Correlation IDs

```rust
let correlation_ids = vec!["order-123".to_string(), "order-456".to_string()];

let workflows_map = workflow_client.get_by_correlation_ids(
    "order_workflow",
    &correlation_ids,
    true,   // include_completed
    false,  // include_tasks
).await?;

for (correlation_id, workflows) in workflows_map {
    println!("{}: {} workflow(s)", correlation_id, workflows.len());
}
```

### Batch Correlation ID Search

```rust
use conductor::client::CorrelationIdsSearchRequest;

let request = CorrelationIdsSearchRequest::new()
    .with_correlation_ids(vec!["order-123".to_string(), "order-456".to_string()])
    .with_workflow_names(vec!["order_workflow".to_string(), "payment_workflow".to_string()]);

let results = workflow_client.get_by_correlation_ids_in_batch(
    &request,
    true,   // include_completed
    true,   // include_tasks
).await?;
```

---

## Advanced Operations

### Skip Task

Skip a pending task in a workflow:

```rust
workflow_client.skip_task(&workflow_id, "task_reference_name").await?;
```

### Update Variables

Update workflow variables during execution:

```rust
use std::collections::HashMap;

let mut variables = HashMap::new();
variables.insert("counter".to_string(), json!(10));
variables.insert("status".to_string(), json!("updated"));

let workflow = workflow_client.update_variables(&workflow_id, variables).await?;
println!("Variables updated: {:?}", workflow.variables);
```

### Update Workflow State

Update workflow state with task result:

```rust
use conductor::client::WorkflowStateUpdate;
use conductor::models::TaskResult;

let update = WorkflowStateUpdate {
    task_reference_name: Some("wait_task".to_string()),
    variables: HashMap::new(),
    task_result: Some(TaskResult {
        status: TaskResultStatus::Completed,
        output_data: [("result".to_string(), json!("done"))].into_iter().collect(),
        ..Default::default()
    }),
};

let run = workflow_client.update_state(
    &workflow_id,
    &update,
    Some(&["next_task".to_string()]),  // wait_until_task_ref_names
    Some(30),                          // wait_for_seconds
).await?;
```

### Test Workflow (Dry Run)

Test a workflow without executing it:

```rust
use conductor::client::TestWorkflowRequest;

let mut mock_outputs = HashMap::new();
mock_outputs.insert("result".to_string(), json!("mocked"));

let request = TestWorkflowRequest::new("my_workflow")
    .with_version(1)
    .with_mock_output("task_ref_1", mock_outputs)
    .with_input([("param".to_string(), json!("value"))].into_iter().collect());

let workflow = workflow_client.test_workflow(&request).await?;
println!("Test result: {:?}", workflow.status);
```

---

## Models Reference

### StartWorkflowRequest

```rust
use conductor::models::StartWorkflowRequest;

let request = StartWorkflowRequest::new("workflow_name")
    .with_version(1)                              // Workflow version
    .with_input_value("key", "value")             // Add single input
    .with_input(input_map)                        // Set all inputs
    .with_correlation_id("correlation-123")       // Correlation ID
    .with_task_to_domain("task_type", "domain")   // Task routing
    .with_priority(5);                            // Execution priority
```

### Workflow

```rust
pub struct Workflow {
    pub workflow_id: String,
    pub workflow_name: String,
    pub workflow_version: i32,
    pub status: WorkflowStatus,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub input: HashMap<String, Value>,
    pub output: HashMap<String, Value>,
    pub variables: HashMap<String, Value>,
    pub tasks: Vec<Task>,
    pub correlation_id: Option<String>,
    pub reason_for_incompletion: Option<String>,
    // ... additional fields
}
```

### WorkflowStatus

```rust
pub enum WorkflowStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Terminated,
    Paused,
}
```

### SearchResult

```rust
pub struct SearchResult<T> {
    pub total_hits: i64,
    pub results: Vec<T>,
}
```

---

## Error Handling

```rust
use conductor::error::ConductorError;

match workflow_client.get_workflow(&workflow_id, true).await {
    Ok(workflow) => {
        println!("Found workflow: {:?}", workflow.status);
    }
    Err(ConductorError::Server { status: 404, message }) => {
        println!("Workflow not found: {}", message);
    }
    Err(ConductorError::Server { status: 409, message }) => {
        println!("Conflict: {}", message);
    }
    Err(ConductorError::Api { message, .. }) => {
        println!("API error: {}", message);
    }
    Err(e) => {
        println!("Other error: {}", e);
    }
}
```

### Common Error Scenarios

| Status | Scenario | Handling |
|--------|----------|----------|
| 404 | Workflow not found | Check workflow_id |
| 409 | Workflow already exists | Use different correlation_id |
| 400 | Invalid request | Check request parameters |
| 500 | Server error | Retry with backoff |

---

## Best Practices

### 1. Use Correlation IDs

```rust
let request = StartWorkflowRequest::new("order_workflow")
    .with_correlation_id(&order_id)  // Use business ID as correlation
    .with_input_value("order_id", &order_id);
```

### 2. Idempotent Workflow Starts

```rust
// Check if workflow already exists for this correlation ID
let existing = workflow_client.get_by_correlation_ids(
    "order_workflow",
    &[order_id.clone()],
    false,  // Don't include completed
    false,
).await?;

if existing.get(&order_id).map(|v| !v.is_empty()).unwrap_or(false) {
    println!("Workflow already running for order {}", order_id);
} else {
    let request = StartWorkflowRequest::new("order_workflow")
        .with_correlation_id(&order_id);
    workflow_client.start_workflow(&request).await?;
}
```

### 3. Proper Error Handling

```rust
async fn start_with_retry(
    client: &WorkflowClient,
    request: &StartWorkflowRequest,
    max_retries: u32,
) -> Result<String> {
    let mut attempt = 0;
    loop {
        match client.start_workflow(request).await {
            Ok(id) => return Ok(id),
            Err(ConductorError::Server { status: 503, .. }) if attempt < max_retries => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

---

## See Also

- [Metadata Management](./METADATA.md) - Workflow definitions
- [Task Management](./TASK_MANAGEMENT.md) - Task operations
- [Worker Implementation](./WORKER.md) - Implementing workers
