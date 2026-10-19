// Copyright 2026 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Use decision inference as a tool inside a normal chat agent.
//! The server compiles the tool into a branchless SWITCH; no Rust worker is needed.
//! `CONDUCTOR_DECISION_PROVIDER=openrouter cargo run --features agents --example decision_tool`
//! Configure `CONDUCTOR_SERVER_URL` and the server-side decision/chat provider credentials.

use conductor::{
    agents::{AgentDef, AgentRuntime, ToolDef},
    configuration::Configuration,
    error::{ConductorError, Result},
};
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = json!({
        "model": std::env::var("CONDUCTOR_DECISION_MODEL").unwrap_or_else(|_| "jev-1.13".to_owned()),
        "questions": {"department": {
            "type": "choice",
            "instructions": "Choose the correct support department.",
            "choices": {
                "billing": "Payments and invoices",
                "technical": "Product errors and troubleshooting"
            }
        }}
    });
    if let Ok(provider) = std::env::var("CONDUCTOR_DECISION_PROVIDER") {
        config["provider"] = Value::String(provider);
    }
    let classify = ToolDef::decision(
        "classify_request",
        "Classify the customer request before answering.",
        json!({"type": "object", "properties": {"state": {"type": "string"}}, "required": ["state"]}),
        serde_json::from_value(config)?,
    );
    let agent = AgentDef::new("decision_tool_support_rust")?
        .with_model(
            std::env::var("CONDUCTOR_AGENT_LLM_MODEL")
                .unwrap_or_else(|_| "openai/gpt-4o-mini".to_owned()),
        )
        .with_instructions("Call classify_request before answering the customer.")
        .with_tool(classify);
    let runtime = AgentRuntime::new(Configuration::from_env())?;
    let handle = runtime
        .start(&agent, "I was charged twice on my latest invoice.".into())
        .await?;
    println!("Execution: {}", handle.execution_id());
    let result = handle.join().await?;
    if !result.is_success() {
        return Err(ConductorError::agent(format!(
            "{}: {:?}",
            result.status, result.error
        )));
    }
    println!("{}", serde_json::to_string_pretty(&result.output)?);
    Ok(())
}
