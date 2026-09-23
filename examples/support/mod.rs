// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::{AgentDef, AgentResult, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

/// Run `agent` to completion while serving its client-side tools from this process.
///
/// `AgentRuntime::run` only starts and polls the execution; tool workers are polled by
/// `AgentRuntime::serve`, which this spawns alongside and aborts once the run finishes.
// Included by every `sdk_playback_*` example; the ones without client-side tools don't call it.
#[allow(dead_code)]
#[allow(clippy::allow_attributes)]
pub async fn run_with_local_tools(
    config: &Configuration,
    agent: &AgentDef,
    prompt: Value,
) -> Result<AgentResult> {
    let mut server_runtime = AgentRuntime::new(config.clone())?;
    let serve_agent = agent.clone();
    let server = tokio::spawn(async move {
        let _ = server_runtime.serve(&serve_agent).await;
    });

    let runtime = AgentRuntime::new(config.clone())?;
    let result = runtime.run(agent, prompt).await;
    server.abort();
    result
}

/// Model every playback example runs against.
///
/// Defaults to the server's recording/playback provider (`mock/mockLLM`), which replays the
/// shared recordings in conductor-oss/conductor's `llm-recordings/`. Set
/// `CONDUCTOR_AGENT_LLM_MODEL` (for example `openai/gpt-4o-mini`) to run the same example
/// against a real provider, for instance to record a new scenario.
pub fn llm_model() -> String {
    std::env::var("CONDUCTOR_AGENT_LLM_MODEL").unwrap_or_else(|_| "mock/mockLLM".to_owned())
}
