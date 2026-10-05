// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{
    AgentDef, FunctionGuardrail, Guardrail, GuardrailResult, OnFail, Position, ToolDef,
};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct OrderArgs {
    order_id: String,
}

#[derive(Deserialize)]
struct CustomerArgs {
    customer_id: String,
}

fn no_pii(content: &str) -> GuardrailResult {
    let cc = regex::Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap();
    let ssn = regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap();
    if cc.is_match(content) || ssn.is_match(content) {
        GuardrailResult::fail(
            "Your response contains PII (credit card or SSN). \
             Redact all card numbers and SSNs before responding.",
        )
    } else {
        GuardrailResult::pass()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let config = Configuration::from_env();

    let get_order_status = ToolDef::function(
        "get_order_status",
        "Look up the current status of an order.",
        json!({
            "type": "object",
            "properties": { "order_id": { "type": "string" } },
            "required": ["order_id"]
        }),
        |args: OrderArgs| async move {
            Ok(json!({
                "order_id": args.order_id,
                "status": "shipped",
                "tracking": "1Z999AA10123456784",
                "estimated_delivery": "2026-02-22"
            }))
        },
    );

    let get_customer_info = ToolDef::function(
        "get_customer_info",
        "Retrieve customer details including payment info on file.",
        json!({
            "type": "object",
            "properties": { "customer_id": { "type": "string" } },
            "required": ["customer_id"]
        }),
        |args: CustomerArgs| async move {
            Ok(json!({
                "customer_id": args.customer_id,
                "name": "Alice Johnson",
                "email": "alice@example.com",
                "card_on_file": "4532-0150-1234-5678",
                "membership": "gold"
            }))
        },
    );

    let no_pii_guardrail = Guardrail::new("no_pii", FunctionGuardrail::new(no_pii))
        .with_position(Position::Output)?
        .with_on_fail(OnFail::Retry)?;

    let agent = AgentDef::new("support_agent")?
        .with_model(model)
        .with_instructions(
            "You are a customer support assistant. Use the available tools to \
             answer questions about orders and customers. Always include all \
             details from the tool results in your response.",
        )
        .with_tool(get_order_status)
        .with_tool(get_customer_info)
        .with_guardrail(no_pii_guardrail);

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let result = runtime
        .run(
            &agent,
            Value::String(
                "I need a full summary: What's the status of order ORD-42, \
             and what's the profile for customer CUST-7?"
                    .into(),
            ),
        )
        .await;
    runtime.shutdown().await?;
    let result = result?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
