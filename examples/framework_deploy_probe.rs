//! One-off probe: exercises AgentRuntime::compile_framework/deploy_framework against a real
//! local Conductor server, using a minimal rawConfig shaped for the server's OpenAINormalizer
//! (confirmed by reading OpenAINormalizer.java directly). Not part of the crate's example set.

use conductor::agents::AgentRuntime;
use conductor::configuration::Configuration;
use conductor::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let raw_config = serde_json::json!({
        "name": "audit_framework_agent",
        "model": "gpt-4o-mini",
        "instructions": "You are a helpful assistant.",
    });

    println!("=== Calling AgentRuntime::compile_framework(\"openai\", ...) ===");
    match runtime.compile_framework("openai", raw_config.clone()).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    println!("\n=== Calling AgentRuntime::deploy_framework(\"openai\", ...) ===");
    match runtime.deploy_framework("openai", raw_config).await {
        Ok(v) => println!("OK: {}", serde_json::to_string_pretty(&v)?),
        Err(e) => println!("ERROR: {e}"),
    }

    println!("\n=== Control: unknown framework should be rejected by the server ===");
    match runtime
        .compile_framework("some_totally_unknown_framework", serde_json::json!({}))
        .await
    {
        Ok(v) => println!("OK (unexpected): {v}"),
        Err(e) => println!("ERROR (expected): {e}"),
    }

    Ok(())
}
