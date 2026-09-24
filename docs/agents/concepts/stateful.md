# Stateful agents

**Audience:** applications that need bounded, in-process conversation history.

## Prerequisites

Decide a message-count budget appropriate for the model's context window before attaching
memory to a long-running agent.

Attach a [`ConversationMemory`] to accumulate and optionally trim message history:

```rust
use conductor::agents::ConversationMemory;

let mut memory = ConversationMemory::new().with_max_messages(50);
memory.add_user_message("What's the weather in Austin?");
memory.add_assistant_message("Sunny, 72F.");

agent = agent.with_memory(memory);
```

`with_max_messages` trims the oldest *non-system* messages first once the limit is exceeded,
keeping every system message in its original position; `max_messages == Some(0)` disables
trimming (same as leaving it unset). Persistence across process restarts is the caller's
responsibility — `ConversationMemory` is a plain in-process accumulator, not itself durable
storage.

## Expected result and failures

`ConversationMemory::to_chat_messages` returns an independent copy — mutating it never affects
the original. Unbounded growth should be handled with `with_max_messages`, not by leaving
history unbounded and hoping the model's context window is large enough.

## Next steps

Read [streaming and approval](streaming-hitl.md) and [runtime modes](deploy-serve-run.md).
