// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct CustomerArgs {
    customer_id: String,
}

#[derive(Deserialize)]
struct InventoryArgs {
    product_id: String,
    #[serde(default = "default_warehouse")]
    warehouse: String,
}

fn default_warehouse() -> String {
    "default".to_owned()
}

#[derive(Deserialize)]
struct OrderArgs {
    order_id: String,
    #[expect(dead_code)]
    action: String,
}

#[derive(Deserialize)]
struct FormatArgs {
    data: Value,
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

    let format_response = ToolDef::function(
        "format_response",
        "Format a data dictionary into a human-readable string.",
        // Allow arbitrary fields in the data object passed to the formatter.
        json!({
            "type": "object",
            "properties": { "data": { "type": "object", "additionalProperties": {} } },
            "required": ["data"]
        }),
        |args: FormatArgs| async move {
            let lines: Vec<String> = args
                .data
                .as_object()
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| format!("  {k}: {}", display(v)))
                        .collect()
                })
                .unwrap_or_default();
            Ok(Value::String(lines.join("\n")))
        },
    );

    let get_customer = ToolDef::function(
        "get_customer",
        "Look up customer details from the CRM system.",
        json!({
            "type": "object",
            "properties": { "customer_id": { "type": "string" } },
            "required": ["customer_id"]
        }),
        |args: CustomerArgs| async move {
            Ok(json!({
                "customer_id": args.customer_id,
                "name": "Example Customer",
                "orders": [{
                    "order_id": "ORD-5678",
                    "customer_id": args.customer_id,
                    "product_id": "PROD-001",
                    "warehouse": "default",
                    "status": "pending"
                }]
            }))
        },
    );

    let check_inventory = ToolDef::function(
        "check_inventory",
        "Check product availability in a warehouse.",
        json!({
            "type": "object",
            "properties": {
                "product_id": { "type": "string" },
                "warehouse": { "type": "string" }
            },
            "required": ["product_id"]
        }),
        |args: InventoryArgs| async move {
            Ok(json!({
                "product_id": args.product_id,
                "warehouse": args.warehouse,
                "in_stock": true,
                "quantity": 12
            }))
        },
    );

    let process_order = ToolDef::function(
        "process_order",
        "Process a customer order. Actions: refund, cancel, update.",
        json!({
            "type": "object",
            "properties": {
                "order_id": { "type": "string" },
                "action": { "type": "string" }
            },
            "required": ["order_id", "action"]
        }),
        |args: OrderArgs| async move {
            Ok(json!({
                "order_id": args.order_id,
                "customer_id": "C-1234",
                "product_id": "PROD-001",
                "warehouse": "default",
                "status": "cancelled"
            }))
        },
    );

    let agent = AgentDef::new("support_agent")?
        .with_model(model)
        .with_instructions(
            "You are a customer support agent. Use the available tools to \
             look up customers, check inventory, process orders, and format \
             responses for the customer.",
        )
        .with_tool(format_response)
        .with_tool(get_customer)
        .with_tool(check_inventory)
        .with_tool(process_order);

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let result = runtime
        .run(
            &agent,
            Value::String(
                "Customer C-1234 wants to cancel order ORD-5678. \
             Look up the customer, check if we have the product in stock, \
             and process the cancellation."
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

fn display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
