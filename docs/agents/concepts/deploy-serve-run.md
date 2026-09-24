# Deploy · Serve · Run

**Audience:** developers choosing a local, CI/CD, or long-lived runtime mode.

## Prerequisites

Create one [`AgentRuntime`] per application lifetime with a configured [`Configuration`].
Call [`AgentRuntime::shutdown`] when it's no longer needed to stop local tool-worker polling.

| Method | Returns | Effect |
|---|---|---|
| `runtime.compile(agent)` | compiled `agentConfig` | Compile only; does not register or execute. |
| `runtime.deploy(agent)` | deployment response | Compile and register; does not execute. |
| `runtime.deploy_with_schedules(agent, schedules)` | deployment response | `deploy`, then reconcile cron schedules — see [scheduling](scheduling.md). |
| `runtime.serve(agent)` | — | Register local tool workers for `agent` (recursively into every sub-agent) and start polling; blocks only long enough to spawn them. |
| `runtime.run(agent, input)` | `AgentResult` | Start and block to a terminal result. |
| `runtime.start(agent, input)` | `AgentHandle` | Start without blocking. |
| `runtime.resume(execution_id, agent)` | `AgentHandle` | Re-register `agent`'s local workers and reattach to an execution this runtime didn't start — typically after a process restart. |
| `runtime.serve_tools(tools)` | — | Register a bare tool list's local workers, for agent shapes with no `AgentDef` to walk. |
| `runtime.shutdown()` | — | Stop every worker `serve`/`serve_tools` registered. |

Use `deploy` in CI/CD, `serve` in long-lived worker processes, and `run` for local
quickstarts. `RunSettings` overrides model/temperature/max-tokens/reasoning-effort for one
`run`/`start` call without mutating the shared `AgentDef`.

## Production pattern

Register/compile with `deploy()` during release, then run `serve()` in one or more long-lived
worker services. Use `compile()` in CI to inspect the compiled workflow before deployment. Use
`start()` for asynchronous callers, and `resume()` after a local worker process restarts and
needs to reattach to executions already running on the server.

## Local tools need a serving process

Tool execution stays local even though the LLM loop itself runs as a durable server-side
workflow. `run`/`start` do **not** implicitly serve local tool workers — an agent with any
locally-invoked tool needs a separate `serve()` call (typically on a second `AgentRuntime`, or
a background task) pointed at the same server for the duration of the run.

## Expected result and cleanup

`compile()`/`deploy()` return without executing anything; `serve()` blocks only long enough to
spawn worker tasks, not for their whole lifetime; `run()` returns a terminal `AgentResult`.
Call `shutdown()` to stop local polling cleanly when a runtime is no longer needed.

## Next steps

See [runtime reference](../reference/runtime.md) and
[control-plane reference](../reference/client.md).
