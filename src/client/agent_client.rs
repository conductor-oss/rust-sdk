// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde_json::Value;

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};

/// Client for the Agent Runtime control-plane API (`/agent/*`).
///
/// Mirrors python-sdk's `conductor.client.agent_client.AgentClient` /
/// `conductor.client.orkes.orkes_agent_client.OrkesAgentClient`, including `stream_sse`
/// (here: [`stream`](Self::stream)) — the response body is handed back as a raw byte stream
/// via [`ApiClient::get_stream`], with SSE framing/decoding into `crate::agents::AgentEvent`
/// left to `crate::agents::AgentStream`.
///
/// Request/response bodies are passed through as raw [`Value`] rather than typed models —
/// there is no typed `AgentExecution`/`AgentStatus`/etc. in this crate yet (those belong to the
/// deferred `AgentRuntime` follow-up), so callers build/inspect JSON directly for now.
#[derive(Clone)]
pub struct AgentClient {
    api: ApiClient,
}

impl AgentClient {
    /// Create a new agent client.
    #[must_use]
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Compile an agent definition into a Conductor workflow, without deploying it.
    /// `POST /agent/compile`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn compile_agent(&self, payload: &Value) -> Result<Value> {
        self.api.post("/agent/compile", payload).await
    }

    /// Compile and register an agent definition as a Conductor workflow.
    /// `POST /agent/deploy`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn deploy_agent(&self, payload: &Value) -> Result<Value> {
        self.api.post("/agent/deploy", payload).await
    }

    /// Start an agent execution.
    /// `POST /agent/start`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn start_agent(&self, payload: &Value) -> Result<Value> {
        self.api.post("/agent/start", payload).await
    }

    /// Get the current status of an agent execution.
    /// `GET /agent/{execution_id}/status`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_status(&self, execution_id: &str) -> Result<Value> {
        let path = format!("/agent/{execution_id}/status");
        self.api
            .get(ApiPath::templated(&path, "/agent/{executionId}/status"))
            .await
    }

    /// Get the full execution record for an agent execution.
    /// `GET /agent/execution/{execution_id}`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_execution(&self, execution_id: &str) -> Result<Value> {
        let path = format!("/agent/execution/{execution_id}");
        self.api
            .get(ApiPath::templated(&path, "/agent/execution/{executionId}"))
            .await
    }

    /// List agent executions matching the given query parameters.
    /// `GET /agent/executions`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn list_executions(&self, params: &[(&str, &str)]) -> Result<Value> {
        self.api.get_with_params("/agent/executions", params).await
    }

    /// Respond to a pending human-in-the-loop request on an agent execution.
    /// `POST /agent/{execution_id}/respond`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn respond(&self, execution_id: &str, body: &Value) -> Result<()> {
        let path = format!("/agent/{execution_id}/respond");
        self.api
            .post_no_response(
                ApiPath::templated(&path, "/agent/{executionId}/respond"),
                body,
            )
            .await
    }

    /// Stop a running agent execution.
    /// `POST /agent/{execution_id}/stop`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn stop(&self, execution_id: &str) -> Result<()> {
        let path = format!("/agent/{execution_id}/stop");
        self.api
            .post_no_body_no_response(ApiPath::templated(&path, "/agent/{executionId}/stop"))
            .await
    }

    /// Send an out-of-band signal message to a running agent execution.
    /// `POST /agent/{execution_id}/signal`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn signal(&self, execution_id: &str, message: &str) -> Result<()> {
        let path = format!("/agent/{execution_id}/signal");
        let body = serde_json::json!({ "message": message });
        self.api
            .post_no_response(
                ApiPath::templated(&path, "/agent/{executionId}/signal"),
                &body,
            )
            .await
    }

    /// Open the Server-Sent Events stream for a running agent execution.
    /// `GET /agent/stream/{execution_id}` (python-sdk: `stream_sse`).
    ///
    /// Returns the raw [`reqwest::Response`]; wrap it in `crate::agents::AgentStream` to decode
    /// SSE frames into `crate::agents::AgentEvent`s.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn stream(&self, execution_id: &str) -> Result<reqwest::Response> {
        let path = format!("/agent/stream/{execution_id}");
        self.api
            .get_stream(ApiPath::templated(&path, "/agent/stream/{executionId}"))
            .await
    }

    /// Push a raw progress/telemetry event for an agent execution.
    /// `POST /agent/events/{execution_id}` (python-sdk's `frameworks/claude_agent_sdk.py`
    /// `_push_event_nonblocking`'s target endpoint). Intended for frameworks that run an opaque
    /// subprocess loop outside Conductor's normal task lifecycle — e.g. the Claude Agent SDK
    /// passthrough transport (`crate::agents::claude_agent_sdk`, `claude-agent-sdk` feature) —
    /// to surface what's happening inside that loop to the Conductor UI/API in near-real-time.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn push_event(&self, execution_id: &str, event: &Value) -> Result<()> {
        let path = format!("/agent/events/{execution_id}");
        self.api
            .post_no_response(
                ApiPath::templated(&path, "/agent/events/{executionId}"),
                event,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_agent_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = AgentClient::new(api);
    }
}
