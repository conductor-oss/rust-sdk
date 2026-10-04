// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass a configured provider/model as the first argument")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("greeter")?
        .with_model(model)
        .with_instructions("You are a friendly assistant. Keep responses brief.");

    let result = runtime.run(&agent, "Say hello.".into()).await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    println!("execution_id: {}", result.execution_id);
    if !result.is_success() {
        return Err(ConductorError::agent(format!(
            "Agent execution ended with status {}",
            result.status
        )));
    }
    Ok(())
}
