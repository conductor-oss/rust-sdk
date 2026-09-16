# Workflow Message Queue (WMQ)

Push a message into a running workflow and consume it from inside the workflow via the
`PULL_WORKFLOW_MESSAGES` system task — a way to signal a running workflow from outside (webhooks,
manual approval, an external system posting a status update) without polling.

## Server requirement

WMQ must be enabled on the target Conductor server:

```properties
conductor.workflow-message-queue.enabled=true
```

If it isn't, `WorkflowClient::send_message` gets back a plain 404 from the server (not a
client-side check) via `ConductorError::Api`.

## Sending a message

```rust
use conductor::client::ConductorClient;
use conductor::configuration::Configuration;
use conductor::models::StartWorkflowRequest;

let client = ConductorClient::new(Configuration::from_env())?;
let workflow_client = client.workflow_client();

// --- start a workflow that has a pull_workflow_messages task in it ---
let workflow_id = workflow_client
    .start_workflow(&StartWorkflowRequest::new("order_processing").with_input_value("orderId", "ORD-42"))
    .await?;

// --- send a message to the running workflow ---
let message_id = workflow_client
    .send_message(
        &workflow_id,
        &serde_json::json!({"event": "payment_confirmed", "amount": 99.99, "currency": "USD"}),
    )
    .await?;
println!("Message enqueued: {message_id}");
```

`send_message` can be called multiple times; each call returns a unique message ID.

## Defining a workflow that receives messages

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("order_processing")
    .with_version(1)
    .with_task(WorkflowTask::pull_workflow_messages("pull_messages", 5)) // up to 5 at a time
    .with_task(
        WorkflowTask::simple("process_message_worker", "process_message")
            .with_input_param("messages", "${pull_messages.output.messages}"),
    );
```

By default `pull_workflow_messages` blocks (stays `IN_PROGRESS`, server re-evaluates roughly
every second) until at least one message is available. Call `.non_blocking()` on the task to
have it complete immediately with an empty `messages`/`count` output instead:

```rust
WorkflowTask::pull_workflow_messages("pull_messages", 5).non_blocking()
```

### Task output shape

```json
{
  "messages": [
    {
      "id": "f3c2a1b0-...",
      "workflowId": "<workflow-instance-id>",
      "payload": { "event": "payment_confirmed", "amount": 99.99 },
      "receivedAt": "2024-01-01T12:00:00Z"
    }
  ],
  "count": 1
}
```

Reference individual fields in a later task's input parameters, e.g.
`"${pull_messages.output.messages[0].payload}"`.

## Error handling

| Error | Cause | What to do |
|---|---|---|
| `ConductorError::Api` (404) | Workflow ID doesn't exist, or WMQ isn't enabled on the server | Verify the workflow was started successfully and the server has the feature flag on |
| `ConductorError::Server` (409) | Workflow isn't `RUNNING` | Check workflow status before sending |
| `ConductorError::Server` (429) | Queue is at capacity (default 1000 messages) | Back off and retry, or increase `conductor.workflow-message-queue.maxQueueSize` |

```rust
match workflow_client.send_message(&workflow_id, &serde_json::json!({"ping": true})).await {
    Ok(message_id) => println!("Sent: {message_id}"),
    Err(e) => eprintln!("Failed to send message: {e}"),
}
```

## Related Documentation

- **[src/client/workflow_client.rs](src/client/workflow_client.rs)** — `WorkflowClient::send_message`
- **[src/models/workflow_def.rs](src/models/workflow_def.rs)** — `WorkflowTask::pull_workflow_messages`
