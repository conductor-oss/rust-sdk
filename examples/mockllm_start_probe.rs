//! One-off probe: exercises AgentRuntime::run() (start + join) end to end against a real local
//! Conductor server, first in LLM record-mode (hitting a local llama.cpp server as the "real"
//! provider) and then in mock/mockLLM replay-mode, to validate the RECORD_MOCKS.md-style
//! record->replay flow actually works for the Rust SDK. Not part of the crate's example set.

use conductor::agents::{AgentDef, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    println!("server_api_url = {}", config.server_api_url);
    let runtime = AgentRuntime::new(config)?;

    let model = std::env::var("PROBE_MODEL").unwrap_or_else(|_| "mock/mockLLM".to_string());
    println!("using model = {model}");

    let agent = AgentDef::new("audit-simple-agent")?
        .with_model(&model)
        .with_instructions("You are a terse assistant. Reply with exactly the words: PROBE OK");

    let input = serde_json::json!({ "prompt": "reply now" });

    println!("=== AgentRuntime::run() ===");
    match runtime.run(&agent, input).await {
        // Now the real, public conductor::agents::AgentResult (Serialize derive) -- this used to
        // fail to compile against runtime.rs's private, unexported duplicate type. Also print
        // is_success()/is_failed() to exercise the new python-matching helper methods.
        Ok(result) => {
            println!("RESULT: {}", serde_json::to_string_pretty(&result)?);
            println!(
                "is_success={} is_failed={}",
                result.is_success(),
                result.is_failed()
            );
        }
        Err(e) => println!("ERROR: {e}"),
    }

    Ok(())
}
