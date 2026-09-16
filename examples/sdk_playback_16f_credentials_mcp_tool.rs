//! Reproduces python-sdk's `examples/agents/16f_credentials_mcp_tool.py` — unlike
//! `04_http_and_mcp_tools`/`16e_credentials_http_tool`, this one has **no** existing recording
//! in the shared `llm-recordings` set from conductor-oss/conductor PR #1614 (only
//! `16e_credentials_http_tool` does). This example both records (against a real model) and
//! replays (against `mock/mockLLM`) itself, controlled by the `PLAYBACK_MODEL` env var
//! (defaults to `mock/mockLLM`), specifically *because* `mcp_tool()`'s tool-call results are
//! parsed/typed MCP content, not a raw HTTP response dump — unlike `http_tool`, nothing here
//! embeds non-deterministic, identity-scoped metadata (no `ETag`, no rate-limit headers), so a
//! recording made against a real model should replay identically in any environment.
//!
//! Setup: `mcp-testkit --transport http --auth <secret>` (this scenario's whole point is
//! demonstrating credential resolution, unlike `04_http_and_mcp_tools`'s no-auth setup) and
//! `CONDUCTOR_SECRET_MCP_API_KEY` set server-side to that same secret.
//!
//! Record: `PLAYBACK_MODEL=anthropic/claude-haiku-4-5-20251001 cargo run --features agents
//! --example sdk_playback_16f_credentials_mcp_tool` against a server with
//! `conductor.ai.record-mode=true`.
//!
//! Replay: `cargo run --features agents --example sdk_playback_16f_credentials_mcp_tool`
//! (model defaults to `mock/mockLLM`) against a server with `conductor.ai.enable-llm-mocks=true`
//! pointed at the directory the recording above was saved into.

use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let model =
        std::env::var("PLAYBACK_MODEL").unwrap_or_else(|_| "mock/mockLLM".to_string());

    let server_url = "http://localhost:3001/mcp";
    let my_mcp_tools = ToolDef::mcp(
        server_url,
        "mcp_tools",
        format!("MCP tools from {server_url}"),
        HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${MCP_API_KEY}".to_string(),
        )]),
        None,
        64,
        vec!["MCP_API_KEY".to_string()],
    )?;

    let agent = AgentDef::new("mcp_cred_agent")?
        .with_model(model)
        .with_instructions("You have access to MCP tools. Use them to help the user.")
        .with_tool(my_mcp_tools);

    let result = runtime
        .run(&agent, Value::String("What tools are available?".into()))
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
