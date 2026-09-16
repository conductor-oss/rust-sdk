// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Strategy};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use support::run_with_local_tools;

fn build_coordinator() -> Result<AgentDef> {
    let quick_check = AgentDef::new("quick_check")?
        .with_model("mock/mockLLM")
        .with_instructions("You provide quick, 1-sentence assessments. Be brief and direct.");

    let market_analyst = AgentDef::new("market_analyst_66")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a market analyst. Analyze the market opportunity: \
         size, growth rate, key players. 3-4 bullet points.",
        );

    let risk_analyst = AgentDef::new("risk_analyst_66")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a risk analyst. Identify the top 3 risks: \
         regulatory, technical, and competitive. 3-4 bullet points.",
        );

    let deep_analysis = AgentDef::new("deep_analysis")?
        .with_model("mock/mockLLM")
        .with_sub_agent(market_analyst)?
        .with_sub_agent(risk_analyst)?
        .with_strategy(Strategy::Parallel)?;

    AgentDef::new("coordinator_66")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a business strategist. Route requests to the right team:\n\
             - quick_check for simple yes/no questions or quick assessments\n\
             - deep_analysis for comprehensive analysis requiring multiple perspectives",
        )
        .with_sub_agent(quick_check)?
        .with_sub_agent(deep_analysis)?
        .with_strategy(Strategy::Handoff)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    println!("=== Scenario 1: Deep analysis (handoff -> parallel group) ===");
    let coordinator1 = build_coordinator()?;
    let result = run_with_local_tools(
        &config,
        &coordinator1,
        Value::String("Provide a deep analysis of entering the AI healthcare market.".into()),
    )
    .await?;
    println!("status: {}", result.status);
    println!("output: {}", result.output);

    println!("\n=== Scenario 2: Quick check (handoff -> single agent) ===");
    let coordinator2 = build_coordinator()?;
    let result2 = run_with_local_tools(
        &config,
        &coordinator2,
        Value::String("Is the mobile app market still growing?".into()),
    )
    .await?;
    println!("status: {}", result2.status);
    println!("output: {}", result2.output);

    Ok(())
}
