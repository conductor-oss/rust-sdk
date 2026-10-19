// Copyright 2026 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Route to one of two chat agents with a decision tool, without a local worker.
//! `cargo run --features agents --example decision_router (compile only)`
//! `CONDUCTOR_DECISION_PROVIDER=openrouter cargo run --features agents --example decision_router -- --run`
//! Configure `CONDUCTOR_SERVER_URL` and the server-side decision/chat provider credentials.

use conductor::{
    agents::{AgentDef, AgentRuntime, Strategy, ToolDef},
    configuration::Configuration,
    error::{ConductorError, Result},
};
use serde_json::{json, Value};

fn support_router() -> Result<AgentDef> {
    let model = std::env::var("CONDUCTOR_AGENT_LLM_MODEL")
        .unwrap_or_else(|_| "openai/gpt-4o-mini".to_owned());
    let billing = AgentDef::new("billing_rust")?
        .with_model(model.clone())
        .with_instructions("Help the customer with billing and invoice questions.");
    let technical = AgentDef::new("technical_rust")?
        .with_model(model)
        .with_instructions("Help the customer troubleshoot product issues.");
    let mut config = json!({
        "model": std::env::var("CONDUCTOR_DECISION_MODEL").unwrap_or_else(|_| "jev-1.13".to_owned()),
        "questions": {"agent": {
            "type": "choice",
            "instructions": "Choose the right support team.",
            "choices": {
                "billing_rust": "Payments and invoices",
                "technical_rust": "Product errors and troubleshooting"
            }
        }}
    });
    if let Ok(provider) = std::env::var("CONDUCTOR_DECISION_PROVIDER") {
        config["provider"] = Value::String(provider);
    }
    let selector = ToolDef::decision(
        "support_selector_rust",
        "Choose the support agent that should handle the request.",
        json!({"type": "object", "properties": {}}),
        serde_json::from_value(config)?,
    );
    AgentDef::new("decision_support_router_rust")?
        .with_decision_router(selector)?
        .with_strategy(Strategy::Router)?
        .with_sub_agent(billing)?
        .with_sub_agent(technical)?
        .with_max_turns(1)
        .map(|agent| agent.with_synthesize(false))
}

#[tokio::main]
async fn main() -> Result<()> {
    let runtime = AgentRuntime::new(Configuration::from_env())?;
    let agent = support_router()?;
    if !std::env::args().any(|arg| arg == "--run") {
        println!(
            "{}",
            serde_json::to_string_pretty(&runtime.compile(&agent).await?)?
        );
        return Ok(());
    }
    runtime.deploy(&agent).await?;
    let handle = runtime
        .start(
            &agent,
            "The customer reports a duplicate charge on the latest invoice.".into(),
        )
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
