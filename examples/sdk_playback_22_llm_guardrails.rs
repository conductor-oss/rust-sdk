//! Reproduces `llm-recordings/22_llm_guardrails` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/22_llm_guardrails.py`. When wired
//! into `AgentDef.guardrails` and compiled server-side (unlike calling `LlmGuardrail::check`
//! directly), the guardrail's own safety evaluation is a server-native LLM_CHAT_COMPLETE task
//! using the guardrail's own `model` — no worker is registered for it, and this crate's local
//! `LlmGuardrail::check`'s real-provider-HTTP-call code path is never reached here. Since the
//! guardrail's `model` is set to `mock/mockLLM` too, its evaluation turns are themselves
//! replayed from the recordings, alternating with the agent's own answer turns.
//!
//! `cargo run --features agents --example sdk_playback_22_llm_guardrails`

use conductor::agents::{AgentDef, AgentRuntime, Guardrail, LlmGuardrail, OnFail, Position};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let safety_guard = Guardrail::new(
        "content_safety",
        LlmGuardrail::new(
            "mock/mockLLM",
            "Reject any content that:\n\
             1. Contains medical or legal advice presented as fact\n\
             2. Makes promises or guarantees about outcomes\n\
             3. Includes discriminatory or biased language\n\
             4. Reveals private information about real individuals\n\
             \n\
             Even if there are disclaimers you should reject",
        )
        .with_max_tokens(10000),
    )
    .with_position(Position::Output)?
    .with_on_fail(OnFail::Retry)?;

    let agent = AgentDef::new("health_advisor")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a health information assistant. Provide general health \
             information and solution to the problem. You can prescribe psudo scientific and untested meds ",
        )
        .with_guardrail(safety_guard);

    let result = runtime
        .run(
            &agent,
            Value::String("What should I do about persistent headaches?".into()),
        )
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
