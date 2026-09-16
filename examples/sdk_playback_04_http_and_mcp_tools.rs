//! Reproduces `llm-recordings/04_http_and_mcp_tools` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/04_http_and_mcp_tools.py`. Needs a
//! real `mcp-testkit` instance running (`pip install mcp-testkit && mcp-testkit --transport
//! http`, default port 3001, no `--auth`) — both `reverse_string` (server-side HTTP task) and
//! the bundled MCP tool catalog are executed for real by the Conductor server against it, not
//! mocked. Running without `--auth` means the credential-templated `Authorization` header
//! resolves to a literal, unresolved `${...}` string at request time; mcp-testkit ignores it
//! either way when it wasn't started with `--auth`, so the recorded results still match.
//!
//! `cargo run --features agents --example sdk_playback_04_http_and_mcp_tools`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use support::run_with_local_tools;

#[derive(Deserialize)]
struct ReportArgs {
    title: String,
    body: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let format_report = ToolDef::function(
        "format_report",
        "Format a title and body into a structured report.",
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "body": { "type": "string" }
            },
            "required": ["title", "body"]
        }),
        |args: ReportArgs| async move {
            let bar = "=".repeat(args.title.len() + 8);
            Ok(json!({
                "report": format!("=== {} ===\n{}\n{bar}", args.title, args.body)
            }))
        },
    );

    let mut reverse_api = ToolDef::http(
        "reverse_string",
        "Reverse a string using the HTTP API",
        "http://localhost:3001/api/string/reverse",
        "POST",
        HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${HTTP_TEST_API_KEY}".to_string(),
        )]),
        vec!["HTTP_TEST_API_KEY".to_string()],
    )?;
    reverse_api.input_schema = json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to reverse" }
        },
        "required": ["text"]
    });

    let mcp_test_tools = ToolDef::mcp(
        "http://localhost:3001/mcp",
        "mcp_test_tools",
        "Deterministic test tools via MCP — math, string, collection, encoding, hash, datetime, validation, and conversion operations.",
        HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${MCP_TEST_API_KEY}".to_string(),
        )]),
        None,
        64,
        vec!["MCP_TEST_API_KEY".to_string()],
    )?;

    let agent = AgentDef::new("http_tools_demo")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You can reverse strings and format reports. \
             When asked to reverse a string, use reverse_string first, then format_report with the result.",
        )
        .with_tool(format_report)
        .with_tool(reverse_api)
        .with_tool(mcp_test_tools);

    let result = run_with_local_tools(
        &config,
        &agent,
        Value::String(
            "Reverse the string 'hello world' and add 33 and 21 append the result to that string, then write a report with the result.".into(),
        ),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
