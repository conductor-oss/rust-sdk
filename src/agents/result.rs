// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Runtime result/status types for agent executions — mirrors python-sdk's
//! `conductor.ai.agents.result.AgentStatus`/`AgentResult` field-for-field (see `result.py`),
//! since those are the source of truth here, not a Rust-native redesign.
//!
//! Two things about python's actual shape that are easy to get wrong by guessing from the type
//! names alone (confirmed by reading `result.py` and `runtime.py`'s `get_status`, not inferred):
//!
//! - The polling snapshot (`AgentStatus`) has **no status enum at all** — it's a raw `status:
//!   str` passed through verbatim from the server's `GET /agent/{executionId}/status` response,
//!   plus `is_complete`/`is_running`/`is_waiting` booleans (`isComplete`/`isRunning`/
//!   `isWaiting` on the wire) and `reason` (from the wire's `reasonForIncompletion` — **not**
//!   `error`; the server never sends a field literally named `error` on this endpoint).
//! - The terminal result's `error` is derived, not read from a dedicated wire field: python sets
//!   it to `status.reason` only `if status.status in ("FAILED", "TERMINATED")`, and leaves it
//!   `None` for every other terminal status (including `TIMED_OUT` — that asymmetry is python's
//!   behavior, reproduced here rather than "fixed", since python is the source of truth).
//!
//! Deliberately out of scope here (python's `AgentResult` also has `correlation_id`, `messages`,
//! `tool_calls`, `token_usage`, `finish_reason`, `sub_results`, `events` — none of which this
//! crate has an extraction path for yet, e.g. no `_extract_tool_calls`/`_extract_token_usage`
//! equivalent). Adding those fields with no way to populate them would just be dead weight;
//! they belong with whatever future work ports that extraction logic.

use serde_json::Value;

use crate::error::Result;

/// Snapshot of an agent execution's status, as returned by `GET /agent/{executionId}/status`.
///
/// Mirrors python-sdk's `AgentStatus` dataclass (`result.py`) field-for-field. Built via
/// [`AgentStatus::from_response`] rather than a strict `#[derive(Deserialize)]` — python's
/// `get_status()` reads each field with `data.get(key, default)`, tolerating a missing or
/// differently-shaped field instead of failing the whole parse, and this does the same.
#[derive(Debug, Clone)]
pub struct AgentStatus {
    /// The execution this status describes. Supplied by the caller (who already knows it),
    /// not read from the response body — matching python, which never relies on the server
    /// echoing `executionId` back on this endpoint.
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
    /// to `"UNKNOWN"` if the response has no `status` field, matching python's
    /// `data.get("status", "UNKNOWN")`.
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
                .to_string(),
            reason: data
                .get("reasonForIncompletion")
                .and_then(Value::as_str)
                .map(str::to_string),
            pending_tool: data.get("pendingTool").cloned(),
        }
    }

    /// `true` once this snapshot is terminal — mirrors python's `_poll_status_until_complete`,
    /// which stops polling on `status.is_complete`.
    pub fn is_terminal(&self) -> bool {
        self.is_complete
    }
}

/// Terminal outcome of an agent execution, returned by `AgentRuntime::run`/`AgentHandle::join`.
///
/// Mirrors the subset of python-sdk's `AgentResult` dataclass this crate can actually populate
/// today — see the module doc for what's deliberately not ported yet.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResult {
    /// The execution this result is for.
    pub execution_id: String,

    /// Final output. `Value::Null` if the execution failed before producing one.
    pub output: Value,

    /// Raw terminal workflow status string (e.g. `"COMPLETED"`, `"FAILED"`, `"TERMINATED"`,
    /// `"TIMED_OUT"`) — same non-enum treatment as [`AgentStatus::status`], for the same reason:
    /// python assigns the raw string straight through without validating it against its
    /// `Status` enum.
    pub status: String,

    /// Error message, present when [`AgentResult::is_failed`] and `status` is specifically
    /// `"FAILED"` or `"TERMINATED"`. `None` for `"TIMED_OUT"` too — matching python's exact
    /// `if status.status in ("FAILED", "TERMINATED")` check, not "fixed" to also cover timeout.
    pub error: Option<String>,
}

impl AgentResult {
    /// Build from a terminal [`AgentStatus`] — mirrors python's `AgentResult(status=status.status,
    /// error=status.reason if status.status in ("FAILED", "TERMINATED") else None, ...)`.
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
        }
    }

    /// `true` iff the execution completed successfully. Mirrors python's
    /// `AgentResult.is_success`.
    pub fn is_success(&self) -> bool {
        self.status == "COMPLETED"
    }

    /// `true` iff the execution ended in failure, termination, or timeout. Mirrors python's
    /// `AgentResult.is_failed`.
    pub fn is_failed(&self) -> bool {
        matches!(self.status.as_str(), "FAILED" | "TERMINATED" | "TIMED_OUT")
    }
}

/// Poll `get_status` (via `poll_once`) until [`AgentStatus::is_terminal`], sleeping
/// `poll_interval` between attempts, then return the terminal [`AgentResult`]. Shared by
/// `AgentHandle::join`/`AgentRuntime::run` so the poll loop has exactly one implementation.
pub(super) async fn poll_until_terminal<F, Fut>(
    poll_interval: std::time::Duration,
    mut poll_once: F,
) -> Result<AgentResult>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<AgentStatus>>,
{
    loop {
        let status = poll_once().await?;
        if status.is_terminal() {
            return Ok(AgentResult::from_status(status));
        }
        tokio::time::sleep(poll_interval).await;
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
        // Matches python: execution_id always comes from the caller-supplied parameter, never
        // from the response body, even if the body happens to carry a (possibly different) one.
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

    /// Regression test for a real python-sdk asymmetry, reproduced deliberately: `TIMED_OUT`
    /// does NOT surface `reason` as `error`, even though it's just as much a reason-bearing
    /// terminal failure as `FAILED`/`TERMINATED`. Python's own `if status.status in ("FAILED",
    /// "TERMINATED")` check simply omits it — don't silently "fix" this on the Rust side, since
    /// that would make the two SDKs disagree on a real execution's `AgentResult.error`.
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

    #[tokio::test]
    async fn test_poll_until_terminal_polls_until_status_is_terminal() {
        let mut calls = 0;
        let result = poll_until_terminal(std::time::Duration::from_millis(1), || {
            calls += 1;
            let call = calls;
            async move {
                if call < 3 {
                    Ok(AgentStatus::from_response("exec-10", &json!({"status": "RUNNING"})))
                } else {
                    Ok(AgentStatus::from_response(
                        "exec-10",
                        &json!({"status": "COMPLETED", "isComplete": true, "output": {"result": "ok"}}),
                    ))
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(calls, 3);
        assert_eq!(result.output, json!({"result": "ok"}));
        assert!(result.is_success());
    }
}
