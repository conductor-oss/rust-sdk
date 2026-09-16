// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, AgentResult, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Deserialize)]
struct AccountArgs {
    account_id: String,
}

#[derive(Deserialize)]
struct TransferArgs {
    from_acct: String,
    to_acct: String,
    // Kept as a raw `Value` rather than `f64` -- python's `transfer_funds(amount: float)` never
    // actually coerces the JSON-decoded argument to a float at runtime (python doesn't enforce
    // type hints), so it passes through whatever numeric type the LLM's tool-call arguments
    // used verbatim (an integer `500`, not `500.0`, per the recording). Deserializing into
    // `f64` here would re-serialize as `500.0` and fail the mock provider's exact-request
    // match on this scenario's third (final) LLM turn.
    amount: Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let check_balance = ToolDef::function(
        "check_balance",
        "Check the balance of an account.",
        json!({
            "type": "object",
            "properties": { "account_id": { "type": "string" } },
            "required": ["account_id"]
        }),
        |args: AccountArgs| async move {
            Ok(json!({ "account_id": args.account_id, "balance": 15000.00 }))
        },
    );

    let transfer_funds = ToolDef::function(
        "transfer_funds",
        "Request a funds transfer; runtime pauses for human approval before execution.",
        json!({
            "type": "object",
            "properties": {
                "from_acct": { "type": "string" },
                "to_acct": { "type": "string" },
                "amount": { "type": "number" }
            },
            "required": ["from_acct", "to_acct", "amount"]
        }),
        |args: TransferArgs| async move {
            Ok(json!({
                "status": "completed",
                "from": args.from_acct,
                "to": args.to_acct,
                "amount": args.amount
            }))
        },
    )
    .with_approval_required(true);

    let agent = AgentDef::new("banker")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a banking assistant. Use check_balance for balance inquiries. \
             When asked to transfer money, first check the balance, then call \
             transfer_funds to request the transfer. The runtime will pause for \
             human approval before the transfer executes.",
        )
        .with_tool(check_balance)
        .with_tool(transfer_funds);

    let mut server_runtime = AgentRuntime::new(config.clone())?;
    let serve_agent = agent.clone();
    let server = tokio::spawn(async move {
        let _ = server_runtime.serve(&serve_agent).await;
    });

    let runtime = AgentRuntime::new(config.clone())?;
    let handle = runtime
        .start(
            &agent,
            Value::String("Transfer $500 from ACC-789 to ACC-456. Check the balance first.".into()),
        )
        .await?;

    let result = loop {
        let status = handle.status().await?;
        if status.is_terminal() {
            break AgentResult::from_status(status);
        }
        if status.is_waiting {
            println!("[human-in-the-loop] approving pending tool call");
            // The recorded session's human reviewer answered "y" for a `reason` field on the
            // approval schema (matching python's script, which asks for every field the
            // response schema declares) -- `handle.approve()` alone sends `{"approved": true}`
            // with no `reason`, which doesn't reproduce the "Human reviewer feedback: Reason:
            // y." message the recording expects, so this uses `respond` directly instead.
            handle
                .respond(&json!({ "approved": true, "reason": "y" }))
                .await?;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    server.abort();

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
