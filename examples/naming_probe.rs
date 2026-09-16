// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime, TerminationCondition, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("audit-hyphen-agent")?
        .with_model("mock/mockLLM")
        .with_tool(ToolDef::function::<Value, _, _>(
            "noop",
            "placeholder",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        ))
        .with_termination(TerminationCondition::stop_message_default());

    let v = runtime.compile(&agent).await?;
    println!("requiredWorkers = {}", v.get("requiredWorkers").unwrap());
    println!("computed task_name (our side) = {}_termination", agent.name);
    Ok(())
}
