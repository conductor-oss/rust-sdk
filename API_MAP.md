# Core API Map

| Need | Rust type | Reference |
|---|---|---|
| Configure transport and auth | `Configuration` | [CONNECTION_AUTHENTICATION.md](CONNECTION_AUTHENTICATION.md) |
| Run workflow executions | `WorkflowClient` | [src/client/workflow_client.rs](src/client/workflow_client.rs) |
| Test a workflow with mocked task outputs | `WorkflowClient::test_workflow` / `TestWorkflowRequest` | [WORKFLOW_TESTING.md](WORKFLOW_TESTING.md) |
| Push/pull messages into a running workflow | `WorkflowClient::send_message` / `WorkflowTask::pull_workflow_messages` | [WORKFLOW_MESSAGE_QUEUE.md](WORKFLOW_MESSAGE_QUEUE.md) |
| Poll and update tasks | `TaskHandler` / `TaskClient` | [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md) |
| Manage workflow/task definitions | `MetadataClient` | [src/client/metadata_client.rs](src/client/metadata_client.rs) |
| Manage workflow/task definitions (Orkes extras: tagging) | `OrkesMetadataClient` | [src/client/orkes_metadata_client.rs](src/client/orkes_metadata_client.rs) |
| Schedule workflows | `SchedulerClient` | [SCHEDULES_EVENTS.md](SCHEDULES_EVENTS.md) |
| Manage schemas | `SchemaClient` | [SCHEMA_CLIENT.md](SCHEMA_CLIENT.md) |
| Manage secrets | `SecretClient` | [SECURITY.md](SECURITY.md) |
| Manage external system integrations | `IntegrationClient` | [src/client/integration_client.rs](src/client/integration_client.rs) |
| Manage event handlers/queue configs | `EventClient` | [SCHEDULES_EVENTS.md](SCHEDULES_EVENTS.md) |
| Manage users/groups/permissions | `AuthorizationClient` | [src/client/authorization_client.rs](src/client/authorization_client.rs) |
| Manage AI prompt templates | `PromptClient` | [src/client/prompt_client.rs](src/client/prompt_client.rs) |
| Compile, deploy, run, signal agents | `AgentClient` / `AgentRuntime` | [docs/agents/README.md](docs/agents/README.md) |
| Automatic long-running-task heartbeats | `WorkerConfig::with_lease_extend_enabled` | [LEASE_EXTENSION.md](LEASE_EXTENSION.md) |
| Metrics and logging | `MetricsCollector` / `tracing` | [OBSERVABILITY.md](OBSERVABILITY.md) |

`ConductorClient` is the single entry point for all of the above — each row's client type has a
matching `client.<x>_client()` accessor (e.g. `client.workflow_client()`).

This crate does not currently expose a workflow-scoped `FileClient` (matching python-sdk); use
the server and task capabilities appropriate to your deployment instead.

## Related Documentation

- **[README.md](README.md)** — full crate overview and feature list
- **[DESIGN.md](DESIGN.md)** — architecture and API documentation
