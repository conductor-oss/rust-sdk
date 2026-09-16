//! Reproduces `llm-recordings/16e_credentials_http_tool` (from conductor-oss/conductor PR
//! #1614). Ported from python-sdk's `examples/agents/16e_credentials_http_tool.py`, with one
//! deliberate deviation: the recorded conversation's tool-result text only depends on the HTTP
//! response body (GitHub's real, still-currently-empty repo list for this URL), not on how the
//! request got authorized — and this test server has no real `GITHUB_TOKEN` credential
//! configured. Sending the literal, unresolved `${GITHUB_TOKEN}` placeholder (or an empty
//! bearer token) makes GitHub return `401`, breaking the match; omitting the
//! credential-templated `Authorization` header entirely reaches the exact same `200` empty-list
//! response the recording expects (GitHub's public repo-list endpoint doesn't require auth).
//! The credential-resolution *mechanism* itself isn't what this playback exercise verifies.
//!
//! This recording's second turn embeds `X-OAuth-Scopes: admin:public_key, gist, read:org,
//! repo, write:packages` and `x-oauth-client-id: 178c6fc778ccc68e1d6a` — the latter happens to
//! equal the `gh` CLI's own OAuth app client ID exactly, meaning whoever recorded this also
//! authenticated via `gh`. Got every header to match byte-for-byte, including scopes and
//! client ID, by refreshing the `gh` CLI's own OAuth grant to the exact recorded scope set
//! (`gh auth refresh --scopes admin:public_key,write:packages --remove-scopes workflow`) and
//! setting `CONDUCTOR_SECRET_GITHUB_TOKEN` to `gh auth token`'s output — confirmed live via a
//! direct `curl` header diff before wiring it in.
//!
//! Even so, this recording is **not reproducible by any real request**: GitHub's response
//! advertises `Vary: Authorization`, and empirically its `ETag` value is scoped to the
//! specific authenticated identity/token making the request, not just the (identical, empty)
//! response body — confirmed by running the exact same request twice with different valid
//! tokens and getting two different ETags for byte-identical content. No token anyone could
//! generate today, however perfectly scoped, can reproduce the literal ETag the original
//! recording session's specific token happened to get. This is the final, unresolvable
//! finding for this recording.
//!
//! `cargo run --features agents --example sdk_playback_16e_credentials_http_tool`

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
            "Accept".to_string(),
            "application/vnd.github.v3+json".to_string(),
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
