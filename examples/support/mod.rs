// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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
// This shared file is included by every `sdk_playback_*` example; the function is actually
// called by most of them but not by `sdk_playback_09_human_in_the_loop`/`_09c_hitl_streaming`
// (no client-side tools there), so `#[expect(dead_code)]` would be "unfulfilled" in the
// examples that do call it. `allow` is the correct choice here, not a stale suppression.
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
