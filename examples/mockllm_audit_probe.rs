//! One-off probe (not part of the crate's normal example set): empirically checks whether
//! `AgentRuntime::compile`/`deploy` send the HTTP body shape the real Conductor server
//! (`/api/agent/compile`) actually expects, and whether a `Strategy::PlanExecute` agent
//! round-trips its `strategy` field. Run against a local server via `cargo run --example
//! mockllm_audit_probe`. Deleted after the audit; not meant to be kept.

use conductor::agents::{AgentConfigSerializer, AgentDef, AgentRuntime, Strategy, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    println!("server_api_url = {}", config.server_api_url);
    let runtime = AgentRuntime::new(config.clone())?;

    let planner = AgentDef::new("planner")?
        .with_model("ggml-org/gemma-4-12B-it-GGUF:Q8_0")
        .with_instructions("Produce a short JSON plan.");

    let coordinator = AgentDef::new("audit-plan-execute-agent")?
        .with_model("ggml-org/gemma-4-12B-it-GGUF:Q8_0")
        .with_instructions("Coordinate via a plan.")
        .with_planner(planner)
        .with_tool(ToolDef::function::<Value, _, _>(
            "noop",
            "a placeholder plan-executable tool",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        ))
        .with_strategy(Strategy::PlanExecute)?;

    let payload = AgentConfigSerializer::serialize(&coordinator);
    println!(
        "\n=== What AgentRuntime::compile() actually sends as the raw HTTP body ===\n{}",
        serde_json::to_string_pretty(&payload)?
    );
    println!(
        "\ntop-level \"strategy\" key present in that body? {}",
        payload.get("strategy").is_some()
    );

    println!("\n=== Calling AgentRuntime::compile(&coordinator) against the real server ===");
    match runtime.compile(&coordinator).await {
        Ok(v) => println!("OK (unexpected): {v}"),
        Err(e) => println!("ERROR (as predicted by the audit): {e}"),
    }

    println!("\n=== Calling AgentRuntime::deploy(&coordinator) against the real server ===");
    match runtime.deploy(&coordinator).await {
        Ok(v) => println!("OK (unexpected): {v}"),
        Err(e) => println!("ERROR (as predicted by the audit): {e}"),
    }

    println!("\n=== Control: manually wrapping the SAME payload as {{\"agentConfig\": ...}} and POSTing directly ===");
    let wrapped: Value = serde_json::json!({ "agentConfig": payload });
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/agent/compile", config.server_api_url))
        .json(&wrapped)
        .send()
        .await
        .map_err(|e| conductor::error::ConductorError::agent(e.to_string()))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    println!("status={status}\nbody={body}");

    Ok(())
}
