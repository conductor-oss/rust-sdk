// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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
