// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

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
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
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
        "Process data locally \u{2014} light retries with linear backoff.",
        json!({
            "type": "object",
            "properties": { "data": { "type": "string" } },
            "required": ["data"]
        }),
        |args: DataArgs| async move { Ok(json!({ "processed": args.data, "status": "ok" })) },
    );

    let agent = AgentDef::new("retry_config_demo")?
        .with_model(model)
        .with_temperature(0.0)
        .with_instructions(
            "You help users fetch and process data. Use the appropriate tool for each request.",
        )
        .with_tool(call_external_api)
        .with_tool(query_database)
        .with_tool(process_data);

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let result = runtime
        .run(
            &agent,
            Value::String("Look up the latest Python release info from the API.".into()),
        )
        .await;
    runtime.shutdown().await?;
    let result = result?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
