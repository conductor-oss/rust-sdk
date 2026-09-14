//! One-off probe: exercises the new plan.rs typed builder + static_plan wire delivery against
//! a real local Conductor server. Not part of the crate's example set.

use conductor::agents::{plan_execute, AgentRuntime, Op, Plan, PlanExecuteOptions, Step, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config.clone())?;

    let tool = ToolDef::function::<Value, _, _>(
        "create_directory",
        "creates a directory",
        serde_json::json!({"type": "object"}),
        |args: Value| async move {
            println!("*** create_directory tool actually invoked with args: {args} ***");
            Ok(serde_json::json!({"created": true}))
        },
    );

    let agent = plan_execute(
        "audit-plan-agent",
        vec![tool],
        PlanExecuteOptions {
            model: Some("mock/mockLLM".to_string()),
            ..Default::default()
        },
    )?;

    let plan = Plan::new(vec![Step::new(
        "setup",
        vec![Op::with_args(
            "create_directory",
            serde_json::json!({"path": "out"}),
        )],
    )]);

    println!(
        "static_plan = {}",
        serde_json::to_string_pretty(&plan.to_value())?
    );

    println!("\n=== Calling AgentRuntime::compile() against the real server ===");
    match runtime.compile(&agent).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    println!("\n=== serve() the create_directory worker in the background ===");
    let mut serve_runtime = AgentRuntime::new(config)?;
    let serve_agent = agent.clone();
    tokio::spawn(async move {
        let _ = serve_runtime.serve(&serve_agent).await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    println!("=== Calling AgentRuntime::run() with static_plan set ===");
    let input = serde_json::json!({"prompt": "run it", "static_plan": plan.to_value()});
    match runtime.run(&agent, input).await {
        Ok(result) => println!("RESULT: {}", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("ERROR: {e}"),
    }

    Ok(())
}
