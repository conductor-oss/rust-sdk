// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Runtime result/status types for agent executions.
//
// AgentStatus::status is a raw string passed through verbatim from the server's
// GET /agent/{executionId}/status response, not parsed into an enum. AgentResult::error is
// derived, not read from a dedicated wire field: it's set to the status's reason only when
// status is "FAILED" or "TERMINATED", and stays None for every other terminal status
// (including "TIMED_OUT").

use serde_json::Value;

/// Snapshot of an agent execution's status, as returned by `GET /agent/{executionId}/status`.
///
/// Built via [`AgentStatus::from_response`], which tolerates missing or differently-shaped
/// fields rather than failing the whole parse.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    /// The execution this status describes. Supplied by the caller, not read from the
    /// response body.
    pub execution_id: String,

    /// `true` once the workflow has reached a terminal state (from the wire's `isComplete`).
    pub is_complete: bool,

    /// `true` while the workflow is still executing (from the wire's `isRunning`).
    pub is_running: bool,

    /// `true` while the workflow is paused, e.g. on a human-in-the-loop step (from the wire's
    /// `isWaiting`).
    pub is_waiting: bool,

    /// Whatever output the execution has produced so far. Populated once terminal; `Value::Null`
    /// before then.
    pub output: Value,

    /// Raw Conductor workflow status string (e.g. `"RUNNING"`, `"COMPLETED"`, `"FAILED"`,
    /// `"TERMINATED"`, `"TIMED_OUT"`) — passed through as-is, not parsed into an enum. Defaults
    /// to `"UNKNOWN"` if the response has no `status` field.
    pub status: String,

    /// Failure/incompletion reason, from the wire's `reasonForIncompletion`. `None` while
    /// running, and also `None` on success.
    pub reason: Option<String>,

    /// A pending tool call awaiting human approval/response, from the wire's `pendingTool`.
    pub pending_tool: Option<Value>,
}

impl AgentStatus {
    /// Build from the raw `GET /agent/{executionId}/status` response body.
    pub fn from_response(execution_id: impl Into<String>, data: &Value) -> Self {
        Self {
            execution_id: execution_id.into(),
            is_complete: data
                .get("isComplete")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_running: data
                .get("isRunning")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_waiting: data
                .get("isWaiting")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            output: data.get("output").cloned().unwrap_or(Value::Null),
            status: data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("UNKNOWN")
                .to_owned(),
            reason: data
                .get("reasonForIncompletion")
                .and_then(Value::as_str)
                .map(str::to_owned),
            pending_tool: data.get("pendingTool").cloned(),
        }
    }

    /// `true` once this snapshot is terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.is_complete
    }
}

/// One tool invocation observed during an agent execution: the tool name, the arguments it was
/// called with, and its result (`None` if the call never got a result).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRecord {
    /// The tool's name.
    pub name: String,
    /// Arguments the tool was called with.
    pub args: Value,
    /// The tool's result, if one was recorded.
    pub result: Option<Value>,
}

/// Terminal outcome of an agent execution, returned by `AgentRuntime::run`/`AgentHandle::join`.
///
/// `tool_calls` is always empty on a result built via [`AgentResult::from_status`].
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResult {
    /// The execution this result is for.
    pub execution_id: String,

    /// Final output. `Value::Null` if the execution failed before producing one.
    pub output: Value,

    /// Raw terminal workflow status string (e.g. `"COMPLETED"`, `"FAILED"`, `"TERMINATED"`,
    /// `"TIMED_OUT"`), passed through as-is.
    pub status: String,

    /// Error message, set when `status` is `"FAILED"` or `"TERMINATED"`. Left `None` for
    /// `"TIMED_OUT"`.
    pub error: Option<String>,

    /// Tool calls observed during the run, in call order. Always empty on a live-poll result.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallRecord>,
}

impl AgentResult {
    /// Build from a terminal [`AgentStatus`].
    #[must_use]
    pub fn from_status(status: AgentStatus) -> Self {
        let error = match status.status.as_str() {
            "FAILED" | "TERMINATED" => status.reason,
            _ => None,
        };
        Self {
            execution_id: status.execution_id,
            output: status.output,
            status: status.status,
            error,
            tool_calls: Vec::new(),
        }
    }

    /// `true` iff the execution completed successfully.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status == "COMPLETED"
    }

    /// `true` iff the execution ended in failure, termination, or timeout.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        matches!(self.status.as_str(), "FAILED" | "TERMINATED" | "TIMED_OUT")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_from_response_reads_running_status() {
        let data = json!({
            "status": "RUNNING",
            "isRunning": true,
        });
        let status = AgentStatus::from_response("exec-1", &data);
        assert_eq!(status.execution_id, "exec-1");
        assert_eq!(status.status, "RUNNING");
        assert!(status.is_running);
        assert!(!status.is_complete);
        assert!(!status.is_waiting);
        assert_eq!(status.output, Value::Null);
        assert!(status.reason.is_none());
        assert!(!status.is_terminal());
    }

    #[test]
    fn test_from_response_reads_waiting_status_with_pending_tool() {
        let data = json!({
            "status": "RUNNING",
            "isWaiting": true,
            "pendingTool": {"name": "send_email", "args": {"to": "a@example.com"}},
        });
        let status = AgentStatus::from_response("exec-2", &data);
        assert!(status.is_waiting);
        assert!(!status.is_terminal());
        assert_eq!(
            status.pending_tool,
            Some(json!({"name": "send_email", "args": {"to": "a@example.com"}}))
        );
    }

    #[test]
    fn test_from_response_reads_completed_status_with_output() {
        let data = json!({
            "status": "COMPLETED",
            "isComplete": true,
            "output": {"result": "PROBE OK"},
        });
        let status = AgentStatus::from_response("exec-3", &data);
        assert!(status.is_terminal());
        assert_eq!(status.output, json!({"result": "PROBE OK"}));
    }

    #[test]
    fn test_from_response_reads_failed_status_with_reason() {
        let data = json!({
            "status": "FAILED",
            "isComplete": true,
            "reasonForIncompletion": "tool call exceeded retry budget",
        });
        let status = AgentStatus::from_response("exec-4", &data);
        assert!(status.is_terminal());
        assert_eq!(
            status.reason.as_deref(),
            Some("tool call exceeded retry budget")
        );
    }

    #[test]
    fn test_from_response_defaults_missing_status_to_unknown() {
        let status = AgentStatus::from_response("exec-5", &json!({}));
        assert_eq!(status.status, "UNKNOWN");
        assert!(!status.is_terminal());
    }

    #[test]
    fn test_from_response_does_not_read_execution_id_from_body() {
        // execution_id always comes from the caller-supplied parameter, never from the response
        // body, even if the body happens to carry a (possibly different) one.
        let status =
            AgentStatus::from_response("exec-caller-supplied", &json!({"executionId": "other"}));
        assert_eq!(status.execution_id, "exec-caller-supplied");
    }

    #[test]
    fn test_agent_result_from_completed_status() {
        let status = AgentStatus::from_response(
            "exec-6",
            &json!({"status": "COMPLETED", "isComplete": true, "output": {"result": "done"}}),
        );
        let result = AgentResult::from_status(status);
        assert_eq!(result.execution_id, "exec-6");
        assert_eq!(result.output, json!({"result": "done"}));
        assert_eq!(result.status, "COMPLETED");
        assert!(result.error.is_none());
        assert!(result.is_success());
        assert!(!result.is_failed());
    }

    #[test]
    fn test_agent_result_surfaces_error_for_failed_status() {
        let status = AgentStatus::from_response(
            "exec-7",
            &json!({"status": "FAILED", "isComplete": true, "reasonForIncompletion": "boom"}),
        );
        let result = AgentResult::from_status(status);
        assert_eq!(result.error.as_deref(), Some("boom"));
        assert!(!result.is_success());
        assert!(result.is_failed());
    }

    #[test]
    fn test_agent_result_surfaces_error_for_terminated_status() {
        let status = AgentStatus::from_response(
            "exec-8",
            &json!({"status": "TERMINATED", "isComplete": true, "reasonForIncompletion": "cancelled by user"}),
        );
        let result = AgentResult::from_status(status);
        assert_eq!(result.error.as_deref(), Some("cancelled by user"));
        assert!(result.is_failed());
    }

    // Regression test: TIMED_OUT does not surface reason as error, even though it's a
    // reason-bearing terminal failure like FAILED/TERMINATED.
    #[test]
    fn test_agent_result_does_not_surface_error_for_timed_out_status() {
        let status = AgentStatus::from_response(
            "exec-9",
            &json!({"status": "TIMED_OUT", "isComplete": true, "reasonForIncompletion": "exceeded timeoutSeconds"}),
        );
        let result = AgentResult::from_status(status);
        assert!(result.error.is_none());
        assert!(result.is_failed());
    }
}
