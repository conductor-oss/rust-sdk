// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, Strategy, SwarmTransition};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let config = Configuration::from_env();

    let refund_agent = AgentDef::new("refund_specialist")?
        .with_model(model.clone())
        .with_instructions(
            "You are a refund specialist. Process the customer's refund request. \
             Check eligibility, confirm the refund amount, and let them know the \
             timeline. Be empathetic and clear. Do NOT ask follow-up questions \u{2014} \
             just process the refund based on what the customer told you.",
        );

    let tech_agent = AgentDef::new("tech_support")?
        .with_model(model.clone())
        .with_instructions(
            "You are a technical support specialist. Diagnose the customer's \
         technical issue and provide clear troubleshooting steps.",
        );

    let support_agent = AgentDef::new("support")?
        .with_model(model)
        .with_instructions(
            "You are the front-line customer support agent. Triage customer requests. \
             If the customer needs a refund, transfer to the refund specialist. \
             If they have a technical issue, transfer to tech support. \
             Use the transfer tools available to you to hand off the conversation.",
        )
        .with_sub_agent(refund_agent)?
        .with_sub_agent(tech_agent)?
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "refund".to_owned(),
            target: "refund_specialist".to_owned(),
        })
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "technical".to_owned(),
            target: "tech_support".to_owned(),
        })
        .with_max_turns(3)?
        .with_strategy(Strategy::Swarm)?;

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&support_agent).await?;
    let result = runtime
        .run(
            &support_agent,
            Value::String(
                "I bought a product last week and it arrived damaged. I want my money back.".into(),
            ),
        )
        .await;
    runtime.shutdown().await?;
    let result = result?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
