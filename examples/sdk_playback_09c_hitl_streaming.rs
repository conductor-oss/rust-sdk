//! Reproduces `llm-recordings/09c_hitl_streaming` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/09c_hitl_streaming.py`, minus the
//! interactive SSE-streaming console loop for the same reason as
//! `sdk_playback_09_human_in_the_loop` — polls `AgentHandle::status` directly and auto-approves
//! (with the same `"y"` reason the recording expects) the moment `is_waiting` is seen.
//!
//! `cargo run --features agents --example sdk_playback_09c_hitl_streaming`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, AgentResult, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Deserialize)]
struct ServiceArgs {
    service_name: String,
}

#[derive(Deserialize)]
struct DeleteArgs {
    service_name: String,
    data_type: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let check_service = ToolDef::function(
        "check_service",
        "Check the health of a service.",
        json!({
            "type": "object",
            "properties": { "service_name": { "type": "string" } },
            "required": ["service_name"]
        }),
        |args: ServiceArgs| async move {
            Ok(json!({ "service": args.service_name, "status": "unhealthy", "uptime": "0m" }))
        },
    );

    let restart_service = ToolDef::function(
        "restart_service",
        "Restart a service. Safe operation, no approval needed.",
        json!({
            "type": "object",
            "properties": { "service_name": { "type": "string" } },
            "required": ["service_name"]
        }),
        |args: ServiceArgs| async move {
            Ok(json!({ "service": args.service_name, "status": "restarted", "new_uptime": "0m" }))
        },
    );

    let delete_service_data = ToolDef::function(
        "delete_service_data",
        "Delete service data. Destructive — requires human approval.",
        json!({
            "type": "object",
            "properties": {
                "service_name": { "type": "string" },
                "data_type": { "type": "string" }
            },
            "required": ["service_name", "data_type"]
        }),
        |args: DeleteArgs| async move {
            Ok(json!({
                "service": args.service_name,
                "data_type": args.data_type,
                "status": "deleted"
            }))
        },
    )
    .with_approval_required(true);

    let agent = AgentDef::new("ops_agent")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are an operations assistant. Work through the request one tool call at a \
             time, in this order:\n\
             1. Check the service with check_service.\n\
             2. If it is unhealthy, restart it with restart_service.\n\
             3. Last, if the user asked you to clear or delete data, call \
             delete_service_data.\n\
             A human approves the deletion, not you — delete_service_data pauses for that \
             approval by itself, so never ask for approval in your own reply.",
        )
        .with_tool(check_service)
        .with_tool(restart_service)
        .with_tool(delete_service_data);

    let mut server_runtime = AgentRuntime::new(config.clone())?;
    let serve_agent = agent.clone();
    let server = tokio::spawn(async move {
        let _ = server_runtime.serve(&serve_agent).await;
    });

    let runtime = AgentRuntime::new(config.clone())?;
    let handle = runtime
        .start(
            &agent,
            Value::String(
                "The payments service is down. Check it, restart it, and clear its stale cache data."
                    .into(),
            ),
        )
        .await?;

    let result = loop {
        let status = handle.status().await?;
        if status.is_terminal() {
            break AgentResult::from_status(status);
        }
        if status.is_waiting {
            println!("[human-in-the-loop] approving pending tool call");
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
