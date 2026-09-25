# Guardrails

**Audience:** authors defining validation and safety controls for agent input, output, or tool
work.

## Prerequisites

Decide whether a failing check should retry, raise, fix, or pause for a human decision. Test
both a passing and a failing value before attaching a guardrail to a live agent.

A [`Guardrail`] wraps a check ([`RegexGuardrail`], [`LlmGuardrail`], or [`FunctionGuardrail`])
plus where it runs ([`Position::Input`]/[`Position::Output`]) and what happens on failure
([`OnFail`]):

```rust
use conductor::agents::{Guardrail, OnFail, RegexGuardrail};

let no_pii = Guardrail::new(
    "no_pii",
    RegexGuardrail::new([r"[\w.+-]+@[\w-]+\.[\w.-]+"])?
        .with_message("Response must not contain email addresses."),
)
.with_on_fail(OnFail::Retry)?;

agent = agent.with_guardrail(no_pii);
```

On `OnFail::Retry`, the guardrail's message is appended to the conversation and the LLM is
called again, up to `max_retries` (default 3). `OnFail::Human` is only valid for
`Position::Output` — an input guardrail runs before the LLM call and has no workflow execution
yet to pause.

## Choose the right check

| Need | Type |
|---|---|
| Deterministic pattern match | `RegexGuardrail` (`RegexMode::Block` or `RegexMode::Allow`) |
| Semantic policy check | `LlmGuardrail` — calls `"openai/<model>"` or `"anthropic/<model>"` directly, synchronously |
| Custom application logic | `FunctionGuardrail`, or implement [`GuardrailCheck`] directly |

Attach a guardrail to an [`AgentDef`](../reference/agent-definition.md) for broad input/output
policy, or to a [`ToolDef`](tools.md) for a check scoped to one tool's own output. A
`FunctionGuardrail`/custom `GuardrailCheck` compiles to a worker task named after the guardrail;
`RegexGuardrail`/`LlmGuardrail` are evaluated without a worker.

## Expected result and failures

Guardrail pass/fail decisions appear as events in the execution (see
[streaming](streaming-hitl.md)). `LlmGuardrail` fails closed on an unsupported provider, a
malformed `"provider/model"` string, a missing API key, or a non-JSON model response — never a
silent pass. Don't send raw secrets or sensitive records into an `LlmGuardrail` policy check;
validate a redacted representation instead.

## Next steps

Pair guardrails with [human approval](streaming-hitl.md), [tool policy](tools.md), and
[termination](termination.md).
