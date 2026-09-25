// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Strategy, ToolDef};
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

#[derive(Deserialize)]
struct ProductArgs {
    product: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

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

    let get_pricing = ToolDef::function(
        "get_pricing",
        "Get pricing information for a product.",
        json!({
            "type": "object",
            "properties": { "product": { "type": "string" } },
            "required": ["product"]
        }),
        |args: ProductArgs| async move {
            Ok(json!({ "product": args.product, "price": 99.99, "discount": "10% off" }))
        },
    );

    let billing_agent = AgentDef::new("billing")?
        .with_model(support::llm_model())
        .with_instructions("You handle billing questions: balances, payments, invoices.")
        .with_tool(check_balance);

    let technical_agent = AgentDef::new("technical")?
        .with_model(support::llm_model())
        .with_instructions("You handle technical questions: order status, shipping, returns.")
        .with_tool(lookup_order);

    let sales_agent = AgentDef::new("sales")?
        .with_model(support::llm_model())
        .with_instructions("You handle sales questions: pricing, products, promotions.")
        .with_tool(get_pricing);

    let support = AgentDef::new("support")?
        .with_model(support::llm_model())
        .with_instructions(
            "Route customer requests to the right specialist: billing, technical, or sales.",
        )
        .with_sub_agent(billing_agent)?
        .with_sub_agent(technical_agent)?
        .with_sub_agent(sales_agent)?
        .with_strategy(Strategy::Handoff)?;

    let result = run_with_local_tools(
        &config,
        &support,
        Value::String("What's the balance on account ACC-123?".into()),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
