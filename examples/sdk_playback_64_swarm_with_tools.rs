//! Reproduces `llm-recordings/64_swarm_with_tools` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/64_swarm_with_tools.py`.
//!
//! `cargo run --features agents --example sdk_playback_64_swarm_with_tools`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Strategy, SwarmTransition, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use support::run_with_local_tools;

#[derive(Deserialize)]
struct AccountArgs {
    account_id: String,
}

#[derive(Deserialize)]
struct OrderArgs {
    order_id: String,
}

fn build_support_agent() -> Result<AgentDef> {
    let check_balance = ToolDef::function(
        "check_balance",
        "Check the balance of a bank account.",
        json!({
            "type": "object",
            "properties": { "account_id": { "type": "string" } },
            "required": ["account_id"]
        }),
        |args: AccountArgs| async move {
            Ok(json!({ "account_id": args.account_id, "balance": 5432.10, "currency": "USD" }))
        },
    );

    let lookup_order = ToolDef::function(
        "lookup_order",
        "Look up the status of an order.",
        json!({
            "type": "object",
            "properties": { "order_id": { "type": "string" } },
            "required": ["order_id"]
        }),
        |args: OrderArgs| async move {
            Ok(json!({ "order_id": args.order_id, "status": "shipped", "eta": "2 days" }))
        },
    );

    let billing_specialist = AgentDef::new("billing_specialist")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a billing specialist. Use the check_balance tool to look up \
             account balances. Include the balance amount in your response.",
        )
        .with_tool(check_balance);

    let order_specialist = AgentDef::new("order_specialist")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are an order specialist. Use the lookup_order tool to check \
             order status. Include the shipping status and ETA in your response.",
        )
        .with_tool(lookup_order);

    AgentDef::new("support")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are front-line customer support. Triage customer requests. \
             Transfer to billing_specialist for account/payment questions, \
             order_specialist for shipping/order questions.",
        )
        .with_sub_agent(billing_specialist)?
        .with_sub_agent(order_specialist)?
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "billing".to_string(),
            target: "billing_specialist".to_string(),
        })
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "order".to_string(),
            target: "order_specialist".to_string(),
        })
        .with_max_turns(3)?
        .with_strategy(Strategy::Swarm)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    println!("=== Scenario 1: Billing question (swarm -> billing + tool) ===");
    let support1 = build_support_agent()?;
    let result = run_with_local_tools(
        &config,
        &support1,
        Value::String("What's the balance on account ACC-456?".into()),
    )
    .await?;
    println!("status: {}", result.status);
    println!("output: {}", result.output);

    println!("\n=== Scenario 2: Order question (swarm -> order + tool) ===");
    let support2 = build_support_agent()?;
    let result2 = run_with_local_tools(
        &config,
        &support2,
        Value::String("Where is my order ORD-789?".into()),
    )
    .await?;
    println!("status: {}", result2.status);
    println!("output: {}", result2.output);

    Ok(())
}
