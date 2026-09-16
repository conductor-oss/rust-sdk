// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime, Strategy};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let market_analyst = AgentDef::new("market_analyst")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a market analyst. Analyze the given topic from a market perspective: \
             market size, growth trends, key players, and opportunities.",
        );

    let risk_analyst = AgentDef::new("risk_analyst")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a risk analyst. Analyze the given topic for risks: \
             regulatory risks, technical risks, competitive threats, and mitigation strategies.",
        );

    let compliance_checker = AgentDef::new("compliance")?
        .with_model("mock/mockLLM")
        .with_instructions(
        "You are a compliance specialist. Check the given topic for compliance considerations: \
             data privacy, regulatory requirements, and industry standards.",
    );

    let analysis = AgentDef::new("analysis")?
        .with_model("mock/mockLLM")
        .with_sub_agent(market_analyst)?
        .with_sub_agent(risk_analyst)?
        .with_sub_agent(compliance_checker)?
        .with_strategy(Strategy::Parallel)?;

    let result = runtime
        .run(
            &analysis,
            Value::String(
                "Launching an AI-powered healthcare diagnostic tool in the US market".into(),
            ),
        )
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
