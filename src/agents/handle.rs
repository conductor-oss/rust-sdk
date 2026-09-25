// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Non-blocking control surface for a running agent execution.
//
// AgentHandle is what AgentRuntime::start returns instead of blocking to a result the
// way AgentRuntime::run does. AgentHandle::approve/AgentHandle::reject/
// AgentHandle::respond always target this handle's own execution_id. Build a separate
// AgentHandle (via AgentClient/AgentHandle::new) over a different execution id if a
// nested sub-execution needs a direct response.

use std::time::Duration;

use serde_json::{json, Value};

use crate::client::AgentClient;
use crate::error::Result;

use super::result::{AgentResult, AgentStatus};
use super::stream::AgentStream;

// Interval between get_status polls in AgentHandle::join.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Non-blocking handle to a running (or already-finished) agent execution.
///
/// Returned by `AgentRuntime::start`. Cheap to clone and to hand to `tokio::spawn` — it holds
/// only a cloned [`AgentClient`] and the execution id, no runtime state of its own.
#[derive(Clone)]
pub struct AgentHandle {
    client: AgentClient,
    execution_id: String,
}

impl AgentHandle {
    /// Build a handle over an already-started execution.
    ///
    /// Called by `AgentRuntime::start` once the server has accepted the execution and handed
    /// back an `execution_id`; not normally constructed directly by SDK users.
    pub fn new(client: AgentClient, execution_id: impl Into<String>) -> Self {
        Self {
            client,
            execution_id: execution_id.into(),
        }
    }

    /// The execution id this handle targets.
    #[must_use]
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Fetch the current status without blocking.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn status(&self) -> Result<AgentStatus> {
        let value = self.client.get_status(&self.execution_id).await?;
        Ok(AgentStatus::from_response(
            self.execution_id.clone(),
            &value,
        ))
    }

    /// Block until the execution reaches a terminal status, then return its [`AgentResult`].
    ///
    /// Polls `GET /agent/{id}/status` on a fixed interval.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn join(&self) -> Result<AgentResult> {
        loop {
            let status = self.status().await?;
            if status.is_terminal() {
                return Ok(AgentResult::from_status(status));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Open a live [`AgentStream`] of [`super::AgentEvent`]s for this execution, over the
    /// server's SSE endpoint.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn stream(&self) -> Result<AgentStream> {
        let response = self.client.stream(&self.execution_id).await?;
        Ok(AgentStream::new(response))
    }

    /// Complete a pending human task with an arbitrary output body. Lower-level than
    /// [`AgentHandle::approve`]/[`AgentHandle::reject`] — use this when the pending step expects
    /// free-form input (e.g. a `ToolType::Human` question) rather than a binary approve/reject
    /// decision.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn respond(&self, body: &Value) -> Result<()> {
        self.client.respond(&self.execution_id, body).await
    }

    /// Approve a pending human-in-the-loop step.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn approve(&self) -> Result<()> {
        self.respond(&approve_payload()).await
    }

    /// Reject a pending human-in-the-loop step with a reason.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn reject(&self, reason: &str) -> Result<()> {
        self.respond(&reject_payload(reason)).await
    }

    /// Gracefully stop the running execution.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn stop(&self) -> Result<()> {
        self.client.stop(&self.execution_id).await
    }
}

fn approve_payload() -> Value {
    json!({ "approved": true })
}

fn reject_payload(reason: &str) -> Value {
    json!({ "approved": false, "reason": reason })
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
        assert_eq!(payload, json!({ "approved": true }));
    }

    #[test]
    fn reject_payload_has_expected_shape() {
        let payload = reject_payload("needs a manager");
        assert_eq!(
            payload,
            json!({ "approved": false, "reason": "needs a manager" })
        );
    }

    #[test]
    fn reject_payload_carries_the_exact_reason_given() {
        let payload = reject_payload("amount over threshold");
        assert_eq!(payload["reason"], "amount over threshold");
    }
}
