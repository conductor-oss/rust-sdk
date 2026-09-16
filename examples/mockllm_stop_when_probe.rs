// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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
