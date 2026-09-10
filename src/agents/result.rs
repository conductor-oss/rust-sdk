// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Runtime result/status types for agent executions — what [`crate::client::AgentClient::get_status`]
//! / `get_execution` return, and the terminal outcome `AgentRuntime::run`/`AgentHandle::join`
//! (added in sibling Wave-4 slices) hand back to the caller. Pure data + `Deserialize`, no
//! dependency on a running `AgentRuntime` — see `docs/agents/parity-plan.md` and
//! `docs/agents/rust-sdk-design.md` for where this sits in the overall design.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ConductorError, Result};

/// Point-in-time execution state of an agent run, as reported by the server.
///
/// Mirrors the `status` field on the JSON `AgentClient::get_status`/`get_execution` return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentExecutionState {
    /// The agent is actively running (including mid-turn, tool dispatch, etc).
    Running,
    /// The agent is paused waiting on a human-in-the-loop response (approve/reject/respond).
    Waiting,
    /// The agent finished successfully; `output` on [`AgentStatus`] is populated.
    Completed,
    /// The agent finished with an error; `error` on [`AgentStatus`] is populated.
    Failed,
}

/// Snapshot of an agent execution's status, as returned by `GET /agent/{executionId}/status`
/// and present on the execution record from `GET /agent/execution/{executionId}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    /// The execution this status describes.
    pub execution_id: String,

    /// Current lifecycle state.
    pub status: AgentExecutionState,

    /// Final output, present once `status` is [`AgentExecutionState::Completed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,

    /// Error message, present once `status` is [`AgentExecutionState::Failed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentStatus {
    /// `true` once the execution has left `Running`/`Waiting` and won't change again —
    /// `AgentRuntime::run`'s poll loop stops on this.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            AgentExecutionState::Completed | AgentExecutionState::Failed
        )
    }

    /// `true` if the execution finished successfully.
    pub fn is_complete(&self) -> bool {
        matches!(self.status, AgentExecutionState::Completed)
    }

    /// `true` if the execution finished with an error.
    pub fn is_failed(&self) -> bool {
        matches!(self.status, AgentExecutionState::Failed)
    }
}

/// Terminal outcome of an agent execution, returned by `AgentRuntime::run`/`AgentHandle::join`
/// (see sibling Wave-4 slices) once polling reaches a terminal [`AgentStatus`].
///
/// `output`/`error` are hoisted to the top level (matching parity-plan.md's `result.output`
/// usage) even though they're also reachable via `status`, since that's the field callers reach
/// for on the success path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResult {
    /// The execution this result is for.
    pub execution_id: String,

    /// Final output. `Value::Null` if the execution failed before producing one.
    #[serde(default)]
    pub output: Value,

    /// The terminal status this result was built from.
    pub status: AgentStatus,

    /// Error message, present if the execution failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AgentResult {
    /// Build a result directly from its parts.
    pub fn new(execution_id: impl Into<String>, output: Value, status: AgentStatus) -> Self {
        let error = status.error.clone();
        Self {
            execution_id: execution_id.into(),
            output,
            status,
            error,
        }
    }
}

impl TryFrom<Value> for AgentResult {
    type Error = ConductorError;

    /// Parse a completed execution record (the `Value` returned by
    /// `AgentClient::get_execution`/`get_status`) into an `AgentResult`.
    fn try_from(value: Value) -> Result<Self> {
        let status: AgentStatus = serde_json::from_value(value)?;
        let output = status.output.clone().unwrap_or(Value::Null);
        Ok(Self::new(status.execution_id.clone(), output, status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_running_status() {
        let value = json!({
            "executionId": "exec-1",
            "status": "RUNNING",
        });
        let status: AgentStatus = serde_json::from_value(value).unwrap();
        assert_eq!(status.execution_id, "exec-1");
        assert_eq!(status.status, AgentExecutionState::Running);
        assert!(status.output.is_none());
        assert!(status.error.is_none());
        assert!(!status.is_terminal());
        assert!(!status.is_complete());
        assert!(!status.is_failed());
    }

    #[test]
    fn test_deserialize_waiting_status() {
        let value = json!({
            "executionId": "exec-2",
            "status": "WAITING",
        });
        let status: AgentStatus = serde_json::from_value(value).unwrap();
        assert_eq!(status.status, AgentExecutionState::Waiting);
        assert!(!status.is_terminal());
    }

    #[test]
    fn test_deserialize_completed_status_with_output() {
        let value = json!({
            "executionId": "exec-3",
            "status": "COMPLETED",
            "output": { "answer": 42 },
        });
        let status: AgentStatus = serde_json::from_value(value).unwrap();
        assert_eq!(status.status, AgentExecutionState::Completed);
        assert_eq!(status.output, Some(json!({ "answer": 42 })));
        assert!(status.is_terminal());
        assert!(status.is_complete());
        assert!(!status.is_failed());
    }

    #[test]
    fn test_deserialize_failed_status_with_error() {
        let value = json!({
            "executionId": "exec-4",
            "status": "FAILED",
            "error": "tool call exceeded retry budget",
        });
        let status: AgentStatus = serde_json::from_value(value).unwrap();
        assert_eq!(status.status, AgentExecutionState::Failed);
        assert_eq!(
            status.error.as_deref(),
            Some("tool call exceeded retry budget")
        );
        assert!(status.is_terminal());
        assert!(!status.is_complete());
        assert!(status.is_failed());
    }

    #[test]
    fn test_agent_result_try_from_completed_execution_record() {
        let value = json!({
            "executionId": "exec-5",
            "status": "COMPLETED",
            "output": { "summary": "done" },
        });
        let result = AgentResult::try_from(value).unwrap();
        assert_eq!(result.execution_id, "exec-5");
        assert_eq!(result.output, json!({ "summary": "done" }));
        assert_eq!(result.status.status, AgentExecutionState::Completed);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_agent_result_try_from_failed_execution_record() {
        let value = json!({
            "executionId": "exec-6",
            "status": "FAILED",
            "error": "boom",
        });
        let result = AgentResult::try_from(value).unwrap();
        assert_eq!(result.execution_id, "exec-6");
        assert_eq!(result.output, Value::Null);
        assert_eq!(result.error.as_deref(), Some("boom"));
        assert!(result.status.is_failed());
    }

    #[test]
    fn test_agent_result_try_from_rejects_malformed_value() {
        let value = json!({ "executionId": "exec-7" });
        assert!(AgentResult::try_from(value).is_err());
    }

    #[test]
    fn test_agent_result_new_pulls_error_from_status() {
        let status = AgentStatus {
            execution_id: "exec-8".into(),
            status: AgentExecutionState::Failed,
            output: None,
            error: Some("nope".into()),
        };
        let result = AgentResult::new("exec-8", Value::Null, status);
        assert_eq!(result.error.as_deref(), Some("nope"));
    }
}
