// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Deserialize)]
struct ReportArgs {
    title: String,
    body: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let config = Configuration::from_env();
    let http_url = std::env::var("CONDUCTOR_EXAMPLE_HTTP_REVERSE_URL").map_err(|error| {
        ConductorError::agent(format!("Set CONDUCTOR_EXAMPLE_HTTP_REVERSE_URL: {error}"))
    })?;
    let mcp_url = std::env::var("CONDUCTOR_EXAMPLE_MCP_URL").map_err(|error| {
        ConductorError::agent(format!("Set CONDUCTOR_EXAMPLE_MCP_URL: {error}"))
    })?;
    let http_credential = std::env::var("CONDUCTOR_EXAMPLE_HTTP_CREDENTIAL").map_err(|error| {
        ConductorError::agent(format!("Set CONDUCTOR_EXAMPLE_HTTP_CREDENTIAL: {error}"))
    })?;
    let mcp_credential = std::env::var("CONDUCTOR_EXAMPLE_MCP_CREDENTIAL").map_err(|error| {
        ConductorError::agent(format!("Set CONDUCTOR_EXAMPLE_MCP_CREDENTIAL: {error}"))
    })?;

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
        http_url,
        "POST",
        HashMap::from([(
            "Authorization".to_owned(),
            format!("Bearer ${{{http_credential}}}"),
        )]),
        vec![http_credential],
    )?;
    reverse_api.input_schema = json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to reverse" }
        },
        "required": ["text"]
    });

    let mcp_test_tools = ToolDef::mcp(
        mcp_url,
        "mcp_test_tools",
        "Deterministic test tools via MCP \u{2014} math, string, collection, encoding, hash, datetime, validation, and conversion operations.",
        HashMap::from([(
            "Authorization".to_owned(),
            format!("Bearer ${{{mcp_credential}}}"),
        )]),
        None,
        64,
        vec![mcp_credential],
    )?;

    let agent = AgentDef::new("http_tools_demo")?
        .with_model(model)
        .with_instructions(
            "You can reverse strings and format reports. \
             When asked to reverse a string, use reverse_string first, then format_report with the result.",
        )
        .with_tool(format_report)
        .with_tool(reverse_api)
        .with_tool(mcp_test_tools);

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let result = runtime.run(&agent, Value::String(
            "Reverse the string 'hello world' and add 33 and 21 append the result to that string, then write a report with the result.".into(),
        )).await;
    runtime.shutdown().await?;
    let result = result?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
