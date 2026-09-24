# AgentClient control-plane reference

**Audience:** applications that need direct control-plane access without going through
`AgentRuntime`.

## Prerequisites

Use a configured `ApiClient`/`Configuration` and ensure any local tools are already served by
a running worker process — `AgentClient` itself never runs tool workers.

`AgentClient` is the thin transport for `/agent/*`. Every method except `stream`/`push_event`
returns a raw `serde_json::Value`; the typed `AgentStatus`/`AgentResult`/`AgentEvent` layer is
built on top of these responses by `AgentRuntime`/`AgentHandle`, not by this client itself.

| Operation | Method |
|---|---|
| Compile, deploy, start | `compile_agent`, `deploy_agent`, `start_agent` |
| Inspect | `get_status`, `get_execution`, `list_executions` |
| Human/control actions | `respond`, `stop`, `signal` |
| Stream events | `stream` (returns a raw `reqwest::Response`; wrap in `AgentStream`) |
| Push progress | `push_event` — for frameworks running an opaque subprocess loop outside Conductor's normal task lifecycle |

`AgentClient::workflow_client()`/`scheduler_client()` return a `WorkflowClient`/
`SchedulerClient` over the same underlying transport, since an agent execution *is* a
Conductor workflow. Use `AgentRuntime` unless an application needs direct control-plane
access without owning local tool workers.

## Expected result and next steps

`stop`, `signal`, and `respond` change a durable execution; authorize callers and make
externally triggered requests idempotent. See [streaming and approval](../concepts/streaming-hitl.md)
and [runtime reference](runtime.md).
