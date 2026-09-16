//! Reproduces `llm-recordings/17_swarm_orchestration` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/17_swarm_orchestration.py`.
//!
//! `cargo run --features agents --example sdk_playback_17_swarm_orchestration`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Strategy, SwarmTransition};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use support::run_with_local_tools;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let refund_agent = AgentDef::new("refund_specialist")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a refund specialist. Process the customer's refund request. \
             Check eligibility, confirm the refund amount, and let them know the \
             timeline. Be empathetic and clear. Do NOT ask follow-up questions — \
             just process the refund based on what the customer told you.",
        );

    let tech_agent = AgentDef::new("tech_support")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a technical support specialist. Diagnose the customer's \
         technical issue and provide clear troubleshooting steps.",
        );

    let support_agent = AgentDef::new("support")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are the front-line customer support agent. Triage customer requests. \
             If the customer needs a refund, transfer to the refund specialist. \
             If they have a technical issue, transfer to tech support. \
             Use the transfer tools available to you to hand off the conversation.",
        )
        .with_sub_agent(refund_agent)?
        .with_sub_agent(tech_agent)?
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "refund".to_string(),
            target: "refund_specialist".to_string(),
        })
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "technical".to_string(),
            target: "tech_support".to_string(),
        })
        .with_max_turns(3)?
        .with_strategy(Strategy::Swarm)?;

    let result = run_with_local_tools(
        &config,
        &support_agent,
        Value::String(
            "I bought a product last week and it arrived damaged. I want my money back.".into(),
        ),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
