# Callbacks

**Audience:** applications that need local observation or lightweight reactions to agent
lifecycle events.

## Prerequisites

Register callback handlers on the `AgentDef` before starting the runtime. Treat callback
payloads as potentially sensitive execution data.

Implement [`CallbackHandler`] and override only the hooks you need — all six default to `None`
("do nothing"):

```rust
use conductor::agents::{CallbackContext, CallbackHandler};
use serde_json::Value;

struct AuditLogger;

#[async_trait::async_trait]
impl CallbackHandler for AuditLogger {
    async fn on_tool_start(&self, ctx: &CallbackContext) -> Option<Value> {
        tracing::info!(tool = ?ctx.get("tool_name"), "tool starting");
        None
    }
}

agent = agent.with_callback(AuditLogger);
```

Each hook receives a [`CallbackContext`] with a different field set (`on_model_start` gets
`messages`, `on_model_end` gets `llm_result`, and so on) and returns `Option<Value>`: `None`
defers to the next handler in the chain, `Some(value)` short-circuits it.

## Expected result and failures

Callbacks receive lifecycle notifications without changing the durable workflow unless a
handler's short-circuit value is meant to affect it. Keep callback work fast and non-blocking;
move durable business effects into a [tool](tools.md) instead. Callbacks are explicitly
non-durable — don't use one as the only record of an audit trail, since a process restart can
interrupt a local observer mid-run.

## Next steps

Use [streaming](streaming-hitl.md) for caller-visible events and [tools](tools.md) for durable
side effects.
