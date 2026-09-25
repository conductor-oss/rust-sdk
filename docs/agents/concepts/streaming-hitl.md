# Streaming and human-in-the-loop

**Audience:** interactive applications that display progress or require a human decision
before a tool executes.

## Prerequisites

Keep the `execution_id` an [`AgentEvent`] carries, not the top-level handle's — nested
sub-executions (handoff/sequential/parallel) put a pending human step in an inner execution.
`AgentHandle::approve`/`reject`/`respond` always target the handle's *own* `execution_id`, so
answering a nested event means building a new handle over that event's id, from the same
`AgentClient` (e.g. `ConductorClient::agent_client()`).

`AgentHandle::stream` yields [`AgentEvent`]s over the server's SSE endpoint:

```rust
let agent_client = conductor_client.agent_client();
let handle = runtime.start(&agent, input).await?;
let mut stream = handle.stream().await?;
while let Some(event) = stream.next().await.transpose()? {
    match event {
        AgentEvent::Waiting { execution_id, pending_tool }
            if pending_tool["tool_name"] == "issue_refund" =>
        {
            // approve/reject target this handle's own execution_id, so build one over the
            // event's id -- it may belong to a nested sub-execution, not the top-level one.
            let inner = AgentHandle::new(agent_client.clone(), execution_id);
            if pending_tool["parameters"]["amount"].as_f64().unwrap_or(0.0) < 100.0 {
                inner.approve().await?;
            } else {
                inner.reject("Needs a manager").await?;
            }
        }
        AgentEvent::Done { output, .. } => println!("{output}"),
        _ => {}
    }
}
```

`AgentEvent` is a tagged enum with 12 variants (`Thinking`, `ToolCall`, `ToolResult`,
`Handoff`, `Waiting`, `GuardrailPass`, `GuardrailFail`, `Error`, `Done`, `ContextCondensed`,
`SubagentStart`, `SubagentStop`) — every variant carries `execution_id`, so there's no code
path that compiles without one in hand. It has no catch-all variant: an unrecognized server
event kind makes `AgentStream::next` return an `Err` and stop iteration.

## Approval pattern

Use a `ToolDef::human` tool or `ToolDef::with_approval_required(true)` for an explicit durable
pause. Resume through [`AgentHandle::respond`]/`approve`/`reject` (or the equivalent
[`AgentClient`](../reference/client.md) calls) rather than relying on in-memory state — the
pause is durable on the server regardless of which process eventually answers it. Make
approval actions idempotent, since callers may retry a request.

"Non-blocking with a callback when done" needs no bespoke API: it's `tokio::spawn` wrapping
`handle.join()`.

## Expected result and cleanup

The stream yields progress events followed by a terminal `Done`/`Error`, while a pending
approval remains visible as waiting work in Conductor until answered. Don't treat streamed
content as final until the terminal event arrives.

## Next steps

See [tools](tools.md), [callbacks](callbacks.md), and
[control-plane reference](../reference/client.md).
