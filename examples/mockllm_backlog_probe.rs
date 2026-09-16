// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime, PrefillToolCall, Strategy, TextGate, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("audit-backlog-agent")?
        .with_model("mock/mockLLM")
        .with_instructions("Coordinate.")
        .with_introduction("Hi, I'm the billing agent.")
        .with_include_contents("none")
        .with_prefill_tools(vec![PrefillToolCall::new(
            "lookup_account",
            serde_json::json!({"id": "abc"}),
        )])
        .with_gate(TextGate::new("DONE").case_insensitive())
        .with_tool(ToolDef::function::<Value, _, _>(
            "lookup_account",
            "looks up an account",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        ));

    println!("=== Calling AgentRuntime::compile() against the real server ===");
    match runtime.compile(&agent).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    // Also exercise the PARALLEL model-inherit path end to end.
    let child = AgentDef::new("child")?.with_model("mock/mockLLM");
    let parallel = AgentDef::new("audit-parallel-agent")?
        .with_sub_agent(child)?
        .with_strategy(Strategy::Parallel)?;
    println!("\nparallel agent inherited model = {:?}", parallel.model);
    println!("=== Calling AgentRuntime::compile() for the PARALLEL agent ===");
    match runtime.compile(&parallel).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    Ok(())
}
