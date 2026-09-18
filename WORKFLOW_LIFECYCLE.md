# Workflow Lifecycle and Versioning

Register a versioned workflow definition, start it through `WorkflowClient`, and inspect its
execution before changing behavior in production.

```rust
let workflow_client = client.workflow_client();
let workflow_id = workflow_client.start_workflow(&request).await?;
let execution = workflow_client.get_workflow(&workflow_id, true).await?; // include_tasks=true
```

## Versioning

Additive output changes are normally safe. Renamed inputs, removed outputs, and changed task
references are breaking and require registering a new `version` on the `WorkflowDef` rather than
mutating an in-use version in place.

## Operational controls

| Operation | Method |
|---|---|
| Pause a running execution | `WorkflowClient::pause_workflow` |
| Resume a paused execution | `WorkflowClient::resume_workflow` |
| Retry from the last failed task | `WorkflowClient::retry_workflow` |
| Terminate with a reason | `WorkflowClient::terminate_workflow` |
| Bulk pause/resume/retry/terminate | `pause_workflows` / `resume_workflows` / `retry_workflows` / `terminate_workflows` |

Use pause/resume for controlled maintenance, retry only transient failures, and always pass an
explicit reason to `terminate_workflow`. Inspect failed tasks (`execution.tasks`, filtering on
`status`/`reason_for_incompletion`) before retrying, to avoid replaying an unsafe side effect a
second time.

## Related Documentation

- **[src/client/workflow_client.rs](src/client/workflow_client.rs)** — the full `WorkflowClient` API
- **[RELIABILITY.md](RELIABILITY.md)** — timeout/retry/idempotency policy
- **[DEBUGGING.md](DEBUGGING.md)** — first checks when an execution isn't behaving as expected
