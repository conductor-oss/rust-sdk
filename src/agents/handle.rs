// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Non-blocking control surface for a running agent execution.
//!
//! [`AgentHandle`] is what `AgentRuntime::start`/`AgentRuntime::resume` return instead of
//! blocking to a result the way `AgentRuntime::run` does (see `docs/agents/parity-plan.md` §
//! "Runtime + Transport" and its example 2). It is a thin, cheaply-`Clone`-able pairing of an
//! [`AgentClient`] with the top-level `execution_id` `start`/`resume` produced — no local
//! polling task, no background thread. Callers choose how to wait on it:
//!
//! - `tokio::spawn(async move { handle.join().await })` for "run in the background, act on the
//!   result later" (no bespoke callback API needed — see parity-plan.md example 2).
//! - `handle.stream()` to observe `AgentEvent`s as they happen and make human-in-the-loop
//!   decisions (`approve`/`reject`/`respond`) at the call site.
//!
//! `execution_id` is a mandatory argument on every HITL method here, never taken implicitly from
//! `self`: `Handoff`/`Sequential`/`Parallel` strategies put the pending `HUMAN` step in a nested
//! sub-execution, so the id a caller must respond against is usually *not* this handle's own
//! top-level execution id — it is whatever id came back on the `AgentEvent::Waiting` (or
//! equivalent) that raised the request. See parity-plan.md § 2 for why this can't be optional.

use std::time::Duration;

use serde_json::{json, Value};

use crate::client::AgentClient;
use crate::error::{ConductorError, Result};

use super::result::{AgentResult, AgentStatus};
use super::stream::AgentStream;

/// Interval between `get_status` polls in [`AgentHandle::join`].
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Non-blocking handle to a running (or already-finished) agent execution.
///
/// Returned by `AgentRuntime::start`/`AgentRuntime::resume`. Cheap to clone and to hand to
/// `tokio::spawn` — it holds only a cloned [`AgentClient`] and the execution id, no runtime
/// state of its own.
#[derive(Clone)]
pub struct AgentHandle {
    client: AgentClient,
    execution_id: String,
}

impl AgentHandle {
    /// Build a handle over an already-started (or resumed) execution.
    ///
    /// Called by `AgentRuntime::start`/`AgentRuntime::resume` once the server has accepted the
    /// execution and handed back an `execution_id`; not normally constructed directly by SDK
    /// users.
    pub fn new(client: AgentClient, execution_id: impl Into<String>) -> Self {
        Self {
            client,
            execution_id: execution_id.into(),
        }
    }

    /// The top-level execution id this handle was constructed with.
    ///
    /// Not necessarily the id to pass to [`AgentHandle::approve`]/[`AgentHandle::reject`]/
    /// [`AgentHandle::respond`] — those target the *event's* execution id, which may be a nested
    /// sub-execution. See the module docs.
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Block until the execution reaches a terminal [`AgentStatus`], then return its
    /// [`AgentResult`].
    ///
    /// Polls `GET /agent/{id}/status` on a fixed interval (same terminal-detection contract as
    /// `AgentRuntime::run`, via [`AgentStatus::is_terminal`]) and fetches the full execution
    /// record with `GET /agent/execution/{id}` once terminal. Intended to be wrapped in
    /// `tokio::spawn` for "run in the background, act on the result later" — see the module
    /// docs and parity-plan.md example 2.
    pub async fn join(&self) -> Result<AgentResult> {
        loop {
            let status_value = self.client.get_status(&self.execution_id).await?;
            let status: AgentStatus = serde_json::from_value(status_value).map_err(|e| {
                ConductorError::agent(format!(
                    "invalid agent status payload for execution {}: {e}",
                    self.execution_id
                ))
            })?;

            if status.is_terminal() {
                let execution_value = self.client.get_execution(&self.execution_id).await?;
                return serde_json::from_value(execution_value).map_err(|e| {
                    ConductorError::agent(format!(
                        "invalid agent execution payload for execution {}: {e}",
                        self.execution_id
                    ))
                });
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Open a live [`AgentStream`] of [`super::AgentEvent`]s for this handle's top-level
    /// execution, over the server's SSE endpoint.
    ///
    /// Does not block and does not consume `self` — a caller can hold the handle and stream it
    /// at the same time (e.g. to call `approve`/`reject` in response to events observed on the
    /// stream, as in parity-plan.md example 2).
    pub fn stream(&self) -> AgentStream {
        AgentStream::new(self.client.clone(), self.execution_id.clone())
    }

    /// Approve a pending human-in-the-loop step.
    ///
    /// `execution_id` must be the execution id the pending request was raised against (see the
    /// module docs) — not necessarily [`AgentHandle::execution_id`].
    pub async fn approve(&self, execution_id: &str) -> Result<()> {
        self.client.respond(execution_id, &approve_payload()).await
    }

    /// Reject a pending human-in-the-loop step with a reason.
    ///
    /// `execution_id` must be the execution id the pending request was raised against (see the
    /// module docs) — not necessarily [`AgentHandle::execution_id`].
    pub async fn reject(&self, execution_id: &str, reason: &str) -> Result<()> {
        self.client
            .respond(execution_id, &reject_payload(reason))
            .await
    }

    /// Send an arbitrary response body to a pending human-in-the-loop step.
    ///
    /// Lower-level than [`AgentHandle::approve`]/[`AgentHandle::reject`] — use this when the
    /// pending step expects free-form input (e.g. a `ToolType::Human` question) rather than a
    /// binary approve/reject decision. `execution_id` must be the execution id the pending
    /// request was raised against (see the module docs) — not necessarily
    /// [`AgentHandle::execution_id`].
    pub async fn respond(&self, execution_id: &str, body: &Value) -> Result<()> {
        self.client.respond(execution_id, body).await
    }
}

// `{"action": "approve"|"reject", "reason": ...}` is this crate's best-effort payload shape for
// `POST /agent/{id}/respond` against a pending HITL step — the exact server-side contract isn't
// pinned down in docs/agents/*.md yet (unlike, e.g., the `agentConfig` shape, which has a schema
// to check against). Revisit against the real server/java-sdk before treating this as load-bearing.
fn approve_payload() -> Value {
    json!({ "action": "approve" })
}

fn reject_payload(reason: &str) -> Value {
    json!({ "action": "reject", "reason": reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ApiClient;
    use crate::Configuration;

    fn test_client() -> AgentClient {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        AgentClient::new(api)
    }

    #[test]
    fn agent_handle_exposes_its_execution_id() {
        let handle = AgentHandle::new(test_client(), "exec-123");
        assert_eq!(handle.execution_id(), "exec-123");
    }

    #[test]
    fn approve_payload_has_expected_shape() {
        let payload = approve_payload();
        assert_eq!(payload, json!({ "action": "approve" }));
    }

    #[test]
    fn reject_payload_has_expected_shape() {
        let payload = reject_payload("needs a manager");
        assert_eq!(
            payload,
            json!({ "action": "reject", "reason": "needs a manager" })
        );
    }

    #[test]
    fn reject_payload_carries_the_exact_reason_given() {
        let payload = reject_payload("amount over threshold");
        assert_eq!(payload["reason"], "amount over threshold");
    }
}
