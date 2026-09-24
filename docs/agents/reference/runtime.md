# AgentRuntime reference

**Audience:** applications owning local tool workers and an agent execution's lifecycle.

## Prerequisites

Create one `AgentRuntime` per application lifetime via `AgentRuntime::new(config)`. Call
`shutdown()` when it's no longer needed.

| Method | Returns | Purpose |
|---|---|---|
| `compile` | compiled `agentConfig` | Compile without registration or execution. |
| `deploy` | deployment response | Register the agent definition, no execution. |
| `deploy_with_schedules` | deployment response | `deploy`, then reconcile cron schedules — see [scheduling](../concepts/scheduling.md). |
| `start` | `AgentHandle` | Start without waiting. |
| `run` | `AgentResult` | Start and wait for completion. |
| `serve` | — | Register local tool workers (recursively into sub-agents) and poll. |
| `resume` | `AgentHandle` | Reattach to an execution this runtime didn't start. |
| `serve_tools` | — | Register a bare tool list's workers, with no `AgentDef` to walk. |
| `task_handler` | `&TaskHandler` | Access the underlying handler, e.g. to call `verify_workers_started`. |
| `shutdown` | — | Stop every worker `serve`/`serve_tools` registered. |
| `compile_framework` / `deploy_framework` / `start_framework` / `run_framework` | — | Framework-marker variants taking `(framework, raw_config)` instead of an `AgentDef`. |

`Configuration::from_env()` reads canonical `CONDUCTOR_*` connection/auth settings. Share one
runtime for an application's lifetime; don't build one per request.

## AgentHandle and AgentResult

`start`/`resume` return an [`AgentHandle`]: `status()` polls once, `join()` blocks to a
terminal [`AgentResult`], `stream()` opens an [`AgentStream`](../concepts/streaming-hitl.md),
and `approve`/`reject`/`respond`/`stop` act on a pending or running execution.
`AgentResult::error` is only populated when `status` is `"FAILED"` or `"TERMINATED"` — it
stays `None` for `"TIMED_OUT"`, since a timeout's reason isn't a completion-time failure
message in the same sense.

## Expected result and common failures

A `SCHEDULED` tool task with no poller means no compatible worker process is serving that
agent's tools — call `serve()`/`serve_tools()` on a process pointed at the same server. A
model error normally means the server-side provider integration is incomplete, not a client
problem.

## Cleanup and next steps

Call `shutdown()` to stop local workers. Continue with [runtime modes](../concepts/deploy-serve-run.md)
and [client control](client.md).
