// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    // In CI the shared playback fixture (conductor-oss/conductor's start-playback-services
    // action) stands in for GitHub at `GITHUB_REPOS_URL`; the real API is the default.
    let repos_url = std::env::var("GITHUB_REPOS_URL").unwrap_or_else(|_| {
        "https://api.github.com/users/Conductor/repos?per_page=5&sort=updated".to_owned()
    });

    // `${GITHUB_TOKEN}` is resolved server-side from the credential store at execution time;
    // the plaintext value never appears in the workflow definition.
    let list_repos = ToolDef::http(
        "list_github_repos",
        "List public GitHub repositories for a user. Returns JSON array with name, url, and stars.",
        repos_url,
        "GET",
        HashMap::from([
            (
                "Authorization".to_owned(),
                "Bearer ${GITHUB_TOKEN}".to_owned(),
            ),
            (
                "Accept".to_owned(),
                "application/vnd.github.v3+json".to_owned(),
            ),
        ]),
        vec!["GITHUB_TOKEN".to_owned()],
    )?;

    let agent = AgentDef::new("github_http_agent")?
        .with_model(support::llm_model())
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
