# Termination conditions

**Audience:** authors bounding cost, time, and unsafe open-ended delegation.

## Prerequisites

Set a meaningful `max_turns` on every agent (default 25) and decide which completion signal is
safe for the operation before adding a `TerminationCondition` on top of it.

[`TerminationCondition`] has four leaf variants and two combinators:

```rust
use conductor::agents::TerminationCondition;

// Stop when the LLM says "DONE" OR after 50 messages.
let stop = TerminationCondition::text_mention("DONE")
    | TerminationCondition::max_message(50)?;

agent = agent.with_termination(stop);
```

| Variant | Fires when |
|---|---|
| `text_mention` / `text_mention_case_sensitive` | The LLM output contains a substring. |
| `stop_message` / `stop_message_default` | The trimmed LLM output exactly equals a string (`stop_message_default` uses `"TERMINATE"`). |
| `max_message` | The conversation reaches a message count. |
| `token_usage` / `max_total_tokens` | Cumulative token usage crosses a configured budget. |

`&`/`\|` compose conditions and flatten automatically — `a & b & c` produces one three-element
`And`, never a nested `And(And(a, b), c)`.

## Stopping a live execution

Use [`AgentHandle::stop`](../reference/runtime.md) or the equivalent
[`AgentClient::stop`](../reference/client.md) to end a running execution directly, independent
of any configured `TerminationCondition`. A stop should be safe to repeat and must not assume
an in-flight external tool call is reversible.

## Expected result and failures

A triggered condition ends the durable execution with its recorded reason available on
[`AgentResult`](../reference/runtime.md). If a tool already started an external side effect,
stopping the agent does not undo it — design compensating work where that matters.

## Next steps

Continue with [multi-agent](multi-agent.md) and [runtime control](../reference/client.md).
