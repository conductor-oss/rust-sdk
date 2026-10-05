// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentResult, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
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
    // Kept as a raw `Value` rather than `f64`: the shared recording has the tool echo the
    // LLM's argument back verbatim (an integer `500`, not `500.0`), and deserializing into
    // `f64` would re-serialize as `500.0` and miss the exact-request match on the final turn.
    amount: Value,
}

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let auto_approve = std::env::args().nth(2).as_deref() == Some("--approve");
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
        .with_model(model)
        .with_instructions(
            "You are a banking assistant. Use check_balance for balance inquiries. \
             When asked to transfer money, first check the balance, then call \
             transfer_funds to request the transfer. The runtime will pause for \
             human approval before the transfer executes.",
        )
        .with_tool(check_balance)
        .with_tool(transfer_funds);

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let handle = runtime
        .start(
            &agent,
            Value::String("Transfer $500 from ACC-789 to ACC-456. Check the balance first.".into()),
        )
        .await?;

    let mut responded = false;
    let result = loop {
        let status = handle.status().await?;
        if status.is_terminal() {
            break AgentResult::from_status(status);
        }
        if status.is_waiting && !responded {
            let approved = if auto_approve {
                true
            } else {
                println!("Approve the pending transfer? [y/N]");
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer).map_err(|error| {
                    ConductorError::agent(format!("Could not read approval: {error}"))
                })?;
                answer.trim().eq_ignore_ascii_case("y")
            };
            handle
                .respond(
                    &json!({ "approved": approved, "reason": if approved { "y" } else { "n" } }),
                )
                .await?;
            responded = true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    runtime.shutdown().await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
