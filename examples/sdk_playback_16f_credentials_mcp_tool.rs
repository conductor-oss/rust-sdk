// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let model = std::env::var("PLAYBACK_MODEL").unwrap_or_else(|_| "mock/mockLLM".to_owned());

    let server_url = "http://localhost:3001/mcp";
    let my_mcp_tools = ToolDef::mcp(
        server_url,
        "mcp_tools",
        format!("MCP tools from {server_url}"),
        HashMap::from([(
            "Authorization".to_owned(),
            "Bearer ${MCP_API_KEY}".to_owned(),
        )]),
        None,
        64,
        vec!["MCP_API_KEY".to_owned()],
    )?;

    let agent = AgentDef::new("mcp_cred_agent")?
        .with_model(model)
        .with_instructions("You have access to MCP tools. Use them to help the user.")
        .with_tool(my_mcp_tools);

    let result = runtime
        .run(&agent, Value::String("What tools are available?".into()))
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
