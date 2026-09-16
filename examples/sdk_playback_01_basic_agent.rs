//! Reproduces `llm-recordings/01_basic_agent` (from conductor-oss/conductor PR #1614) so the
//! shared recording can be replayed through `mock/mockLLM`. Ported field-for-field from
//! python-sdk's `examples/agents/01_basic_agent.py`; the recorded request is just a system +
//! user message with no tools, so nothing else needs to match.
//!
//! Run against a server with `conductor.ai.enable-llm-mocks=true` and
//! `conductor.ai.recordings-directory` pointed at that `llm-recordings` checkout:
//! `cargo run --features agents --example sdk_playback_01_basic_agent`

use conductor::agents::{AgentDef, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("greeter")?
        .with_model("mock/mockLLM")
        .with_instructions("You are a friendly assistant. Keep responses brief.");

    let result = runtime
        .run(
            &agent,
            "Say hello and tell me a fun fact about Python.".into(),
        )
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
