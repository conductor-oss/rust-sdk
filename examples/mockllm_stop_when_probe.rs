//! One-off probe: exercises the new stop_when wire field + worker registration against a real
//! local Conductor server. Not part of the crate's example set.

use conductor::agents::{AgentDef, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("audit-stop-when-agent")?
        .with_model("mock/mockLLM")
        .with_instructions("Coordinate.")
        .with_stop_when(|context: Value| async move {
            Ok(context["iteration"].as_i64().unwrap_or(0) >= 3)
        });

    println!("=== Calling AgentRuntime::compile() against the real server ===");
    match runtime.compile(&agent).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    Ok(())
}
