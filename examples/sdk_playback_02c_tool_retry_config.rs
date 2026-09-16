//! Reproduces `llm-recordings/02c_tool_retry_config` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/02c_tool_retry_config.py` — the
//! per-tool `retry_policy`/`retry_count`/`retry_delay_seconds` settings that example
//! demonstrates aren't part of the recorded LLM request (they're server-side task-definition
//! config, only observable if a tool call actually fails and is retried), and this crate's
//! `ToolDef` has no equivalent builder yet, so they're only noted here, not reproduced. The
//! recorded conversation never actually hits a failure — the mocked model just calls
//! `call_external_api` repeatedly with different phrasings across 11 turns before answering —
//! so it doesn't matter for replay.
//!
//! `cargo run --features agents --example sdk_playback_02c_tool_retry_config`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use support::run_with_local_tools;

#[derive(Deserialize)]
struct QueryArgs {
    query: String,
}

#[derive(Deserialize)]
struct SqlArgs {
    sql: String,
}

#[derive(Deserialize)]
struct DataArgs {
    data: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let call_external_api = ToolDef::function(
        "call_external_api",
        "Call an unreliable external API that may need aggressive retries.",
        json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        }),
        |args: QueryArgs| async move {
            Ok(json!({ "result": format!("Data for: {}", args.query), "source": "external_api" }))
        },
    );

    let query_database = ToolDef::function(
        "query_database",
        "Run a database query with fixed-interval retries for transient connection issues.",
        json!({
            "type": "object",
            "properties": { "sql": { "type": "string" } },
            "required": ["sql"]
        }),
        |args: SqlArgs| async move { Ok(json!({ "rows": [{"id": 1, "value": args.sql}], "count": 1 })) },
    );

    let process_data = ToolDef::function(
        "process_data",
        "Process data locally — light retries with linear backoff.",
        json!({
            "type": "object",
            "properties": { "data": { "type": "string" } },
            "required": ["data"]
        }),
        |args: DataArgs| async move { Ok(json!({ "processed": args.data, "status": "ok" })) },
    );

    let agent = AgentDef::new("retry_config_demo")?
        .with_model("mock/mockLLM")
        .with_temperature(0.0)
        .with_instructions(
            "You help users fetch and process data. Use the appropriate tool for each request.",
        )
        .with_tool(call_external_api)
        .with_tool(query_database)
        .with_tool(process_data);

    let result = run_with_local_tools(
        &config,
        &agent,
        Value::String("Look up the latest Python release info from the API.".into()),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
