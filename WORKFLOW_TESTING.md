# Workflow Testing API

Run a workflow definition against a real Conductor server with mocked task outputs, without
needing real workers to be online — useful for CI and for validating a workflow's branching
logic (switch/decision, fork/join, retries) before deploying it.

This is a server-side feature (`POST /workflow/test`): the server executes the workflow exactly
as it would in production, substituting your mocked outputs wherever a mocked task reference
would otherwise run for real.

## Quick Start

```rust
use conductor::{
    client::{ConductorClient, TestWorkflowRequest},
    configuration::Configuration,
    error::Result,
    models::{WorkflowDef, WorkflowTask},
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ConductorClient::new(Configuration::from_env())?;
    let workflow_client = client.workflow_client();

    let workflow = WorkflowDef::new("order_processing")
        .with_version(1)
        .with_task(WorkflowTask::simple("validate_order", "validate_ref"))
        .with_task(WorkflowTask::simple("charge_payment", "payment_ref"));

    let request = TestWorkflowRequest::new("order_processing")
        .with_workflow_def(workflow)
        .with_input(HashMap::from([("orderId".to_owned(), serde_json::json!("ORD-42"))]))
        .with_mock_output(
            "validate_ref",
            HashMap::from([("valid".to_owned(), serde_json::json!(true))]),
        )
        .with_mock_output(
            "payment_ref",
            HashMap::from([("paymentId".to_owned(), serde_json::json!("PAY-1"))]),
        );

    let execution = workflow_client.test_workflow(&request).await?;
    assert!(execution.is_successful());
    println!("Test run: {}", execution.workflow_id);
    Ok(())
}
```

## `TestWorkflowRequest`

```rust
pub struct TestWorkflowRequest {
    pub name: String,
    pub version: Option<i32>,
    pub input: HashMap<String, serde_json::Value>,
    pub correlation_id: Option<String>,
    pub task_to_domain: HashMap<String, String>,
    pub workflow_def: Option<WorkflowDef>,               // inline def; omit to test an already-registered one
    pub external_input_payload_storage_path: Option<String>,
    pub priority: i32,
    pub task_ref_to_mock_output: HashMap<String, Vec<TaskMock>>,
    pub sub_workflow_test_request: HashMap<String, TestWorkflowRequest>,
}
```

Build one with `TestWorkflowRequest::new(name)` plus builder methods: `with_version`,
`with_workflow_def`, `with_input`, `with_correlation_id`, `with_task_to_domain`,
`with_priority`, `with_mock_output`, `with_mock_outputs`, `with_sub_workflow_test_request`.

### Mocking a single output

`with_mock_output(task_ref, output)` is the common case — one `COMPLETED` attempt:

```rust
request.with_mock_output(
    "check_inventory_ref",
    HashMap::from([("available".to_owned(), serde_json::json!(true))]),
)
```

### Simulating retries and failure statuses

Each task reference maps to a **sequence** of [`TaskMock`]s, not a single output — calling
`with_mock_output` again for the same reference appends another attempt to the sequence. Use
`TaskMock`/`with_mock_outputs` directly for non-`COMPLETED` attempts or timing simulation:

```rust
use conductor::client::TaskMock;
use conductor::models::TaskResultStatus;

let request = TestWorkflowRequest::new("order_processing")
    .with_mock_outputs(
        "payment_ref",
        vec![
            // First attempt: fails, triggering a retry.
            TaskMock::new(TaskResultStatus::Failed, HashMap::new()),
            // Second attempt: succeeds.
            TaskMock::completed(HashMap::from([
                ("paymentId".to_owned(), serde_json::json!("PAY-1")),
            ]))
            .with_execution_time(1200), // simulate a slow attempt, e.g. for timeout testing
        ],
    );
```

`TaskMock::status` accepts any [`TaskResultStatus`] (`Completed`, `Failed`,
`FailedWithTerminalError`, `InProgress`, `Canceled`).

### Mocking sub-workflows

If the workflow under test has a `SubWorkflow` task, mock its internals the same way via
`with_sub_workflow_test_request(task_ref, nested_request)`, where `nested_request` is itself a
full `TestWorkflowRequest` (recursively, for sub-workflows of sub-workflows).

## Inspecting the result

`test_workflow` returns a [`Workflow`](src/models/workflow.rs) — the same type
`WorkflowClient::get_workflow` returns — so every normal inspection applies:

```rust
assert!(execution.is_successful());
assert_eq!(execution.output["trackingNumber"], serde_json::json!("TRACK-12345"));

let payment_attempts: Vec<_> = execution
    .tasks
    .iter()
    .filter(|t| t.reference_task_name == "payment_ref")
    .collect();
assert_eq!(payment_attempts.len(), 2); // failed attempt + retry
```

## Testing workers directly (no server needed)

For unit-testing a worker function's own logic (not the workflow's branching), there's no
special harness — call it like any other async Rust function with a hand-built [`Task`]:

```rust
use conductor::models::Task;

#[tokio::test]
async fn test_process_order_worker() {
    let task = Task {
        task_id: "test-1".to_owned(),
        input_data: HashMap::from([("orderId".to_owned(), serde_json::json!("ORD-1"))]),
        ..Default::default()
    };

    let output = process_order(&task).await.unwrap();
    assert_eq!(output.get("status"), Some(&serde_json::json!("processed")));
}
```

See [TESTING.md](TESTING.md) for running this crate's own test suite (including the
`Integration Tests` CI job, which exercises `test_workflow` against a real OSS Conductor
container).

## Related Documentation

- **[src/client/workflow_client.rs](src/client/workflow_client.rs)** — `TestWorkflowRequest`, `TaskMock`, `WorkflowClient::test_workflow`
- **[examples/test_workflows.rs](examples/test_workflows.rs)** — a complete worked example
- **[TESTING.md](TESTING.md)** — running the SDK's own test suite against a live server
