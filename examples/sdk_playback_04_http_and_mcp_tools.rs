// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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
            "Authorization".to_owned(),
            "Bearer ${HTTP_TEST_API_KEY}".to_owned(),
        )]),
        vec!["HTTP_TEST_API_KEY".to_owned()],
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
        "Deterministic test tools via MCP \u{2014} math, string, collection, encoding, hash, datetime, validation, and conversion operations.",
        HashMap::from([(
            "Authorization".to_owned(),
            "Bearer ${MCP_TEST_API_KEY}".to_owned(),
        )]),
        None,
        64,
        vec!["MCP_TEST_API_KEY".to_owned()],
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
