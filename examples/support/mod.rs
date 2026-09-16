//! Shared helper for the `sdk_playback_*` examples (not a standalone example itself — included
//! via `#[path = "playback_common.rs"] mod playback_common;`). These examples replay
//! `llm-recordings` (from conductor-oss/conductor PR #1614, "Share SDK playback recordings")
//! through `mock/mockLLM` against a locally running server with
//! `conductor.ai.enable-llm-mocks=true`.

use conductor::agents::{AgentDef, AgentResult, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

/// `AgentRuntime::run` only starts an execution and polls it — unlike python-sdk's `run()`, it
/// does not also register/poll local tool workers (`AgentRuntime::serve`) for the duration of
/// the call. Every playback example that uses a client-side tool has to do that wiring itself:
/// spawn `serve` on a second runtime instance pointed at the same server, `run` to completion,
/// then abort the spawned poller. Confirmed against python-sdk's `runtime.py::run`, which calls
/// `self._prepare_workers(...)` internally right after starting — a real, currently-undocumented
/// parity gap in this crate's `AgentRuntime::run`, not something specific to these examples.
#[allow(dead_code)]
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
