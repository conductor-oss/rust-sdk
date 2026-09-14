//! One-off probe: confirms the server's requiredWorkers for a termination-bearing agent match
//! the {name}_termination task name AgentRuntime::serve() now registers, then actually runs the
//! agent end to end with serve() active to prove the worker gets polled and answered correctly.
//! Not part of the crate's example set.

use conductor::agents::{AgentDef, AgentRuntime, TerminationCondition, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config.clone())?;

    let model = std::env::var("PROBE_MODEL").unwrap_or_else(|_| "mock/mockLLM".to_string());
    let agent = AgentDef::new("audit-termination-agent")?
        .with_model(model)
        .with_instructions("Reply with exactly the single word: TERMINATE")
        .with_tool(ToolDef::function::<Value, _, _>(
            "noop",
            "a placeholder tool",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        ))
        .with_termination(TerminationCondition::stop_message_default());

    println!("=== Calling AgentRuntime::compile() ===");
    match runtime.compile(&agent).await {
        Ok(v) => {
            let required = v.get("requiredWorkers").cloned().unwrap_or(Value::Null);
            println!("requiredWorkers = {required}");
        }
        Err(e) => println!("ERROR: {e}"),
    }

    println!("\n=== deploy() to register tool defs ===");
    match runtime.deploy(&agent).await {
        Ok(v) => println!("deploy OK: {v}"),
        Err(e) => println!("deploy ERROR: {e}"),
    }

    println!("\n=== serve() in the background, then run() ===");
    let mut serve_runtime = AgentRuntime::new(config)?;
    let serve_agent = agent.clone();
    tokio::spawn(async move {
        let _ = serve_runtime.serve(&serve_agent).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    match runtime.run(&agent, serde_json::json!({"prompt": "go"})).await {
        Ok(result) => println!("RESULT: {}", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("ERROR: {e}"),
    }

    Ok(())
}
