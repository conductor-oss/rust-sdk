// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let list_repos = ToolDef::http(
        "list_github_repos",
        "List public GitHub repositories for a user. Returns JSON array with name, url, and stars.",
        "https://api.github.com/users/Conductor/repos?per_page=5&sort=updated",
        "GET",
        HashMap::from([(
            "Accept".to_owned(),
            "application/vnd.github.v3+json".to_owned(),
        )]),
        Vec::new(),
    )?;

    let agent = AgentDef::new("github_http_agent")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You list GitHub repos using the list_github_repos tool. Summarize the results.",
        )
        .with_tool(list_repos);

    let result = runtime
        .run(&agent, Value::String("List the repos for Conductor".into()))
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
