// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Deterministic agent testing without an LLM or a live Conductor server -- ports the core,
//! most-portable slice of python-sdk's `conductor.ai.agents.testing` package: [`mock_run`]
//! (`testing/mock.py`'s `mock_run`) and the fluent [`expect`] assertion API (`testing/expect.py`).
//!
//! **Not ported here, deferred to future work:** record/replay of real execution traces
//! (`testing/recording.py`), LLM-judge semantic assertions (`testing/semantic.py`, needs an
//! actual LLM client this crate doesn't have one of yet), per-strategy structural validators
//! (`testing/strategy_validators.py`), the LLM-backed correctness eval runner
//! (`testing/eval_runner.py`), and the pytest plugin (`testing/pytest_plugin.py` -- N/A as
//! designed; this crate's equivalent is just `#[test]`/`cargo test`, no plugin needed). See
//! `docs/agents/README.md`'s Testing section for these as their own tracked follow-ups.
//!
//! ## Narrower than python's version, and why
//!
//! Python's [`ScriptedEvent`]-equivalent (`MockEvent`) covers ten event kinds (`THINKING`,
//! `TOOL_CALL`, `TOOL_RESULT`, `HANDOFF`, `MESSAGE`, `GUARDRAIL_PASS`, `GUARDRAIL_FAIL`,
//! `WAITING`, `DONE`, `ERROR`) because python's real SSE stream consumer (`runtime.py`'s
//! `stream()`) actually distinguishes all of them from the wire. This crate's [`super::stream`]
//! only recognizes five real wire event kinds (`Message`/`Progress`/`Waiting`/`Done`/`Error`) --
//! confirmed by reading the actual server payload shape, not guessed -- so [`ScriptedEvent`]
//! only covers what has a real counterpart today: tool calls/results, completion, and failure.
//! Tool-call tracking doesn't need its own [`super::AgentEvent`] variant either way -- a
//! [`crate::agents::result::ToolCallRecord`] is populated directly from the script, independent
//! of whatever the wire actually emits for it.

use serde_json::Value;

use super::credentials::Credentials;
use super::def::AgentDef;
use super::result::{AgentResult, ToolCallRecord};
use super::tool::ToolContext;

/// One step of a scripted mock execution, passed to [`mock_run`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptedEvent {
    /// The agent calls a tool. If `agent` has a matching tool with a real handler and no
    /// explicit [`ScriptedEvent::ToolResult`] immediately follows, [`mock_run`] calls that
    /// handler for real and records its output -- matching python's `auto_execute_tools=True`
    /// default. Pass `args` as `serde_json::json!({...})` (an object).
    ToolCall { name: String, args: Value },
    /// An explicit result for the preceding [`ScriptedEvent::ToolCall`], skipping real handler
    /// invocation for it -- use this to script a specific tool outcome without needing (or
    /// wanting) the handler to actually run.
    ToolResult { name: String, result: Value },
    /// The execution completes successfully with this output.
    Done { output: Value },
    /// The execution fails with this message.
    Error { message: String },
}

/// Build an [`AgentResult`] from a scripted event sequence -- no LLM call, no server round trip.
///
/// Walks `script` in order. On a [`ScriptedEvent::ToolCall`] for a tool `agent` actually has a
/// handler for, calls that handler for real (with default [`Credentials`]/[`ToolContext`]) and
/// records its output as the tool call's result, *unless* the very next scripted event is an
/// explicit [`ScriptedEvent::ToolResult`] for the same tool, which takes precedence. A
/// `ToolCall` for a tool `agent` doesn't have (or that has no handler, e.g. an `http`/`mcp`
/// tool) is recorded with `result: None` unless a following `ToolResult` supplies one.
///
/// `prompt` isn't sent anywhere (there's no LLM call) and is currently unused -- it's still a
/// parameter so call sites read naturally (`mock_run(&agent, "the prompt", script)`) and so a
/// future extension (e.g. recording it on the result) doesn't need a signature change.
pub async fn mock_run(agent: &AgentDef, _prompt: &str, script: Vec<ScriptedEvent>) -> AgentResult {
    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();
    let mut output = Value::Null;
    let mut status = "COMPLETED".to_owned();
    let mut error = None;

    let mut i = 0;
    while i < script.len() {
        match &script[i] {
            ScriptedEvent::ToolCall { name, args } => {
                // An explicit ToolResult for this same tool immediately following takes
                // precedence over auto-executing the real handler.
                if let Some(ScriptedEvent::ToolResult {
                    name: result_name,
                    result,
                }) = script.get(i + 1)
                {
                    if result_name == name {
                        tool_calls.push(ToolCallRecord {
                            name: name.clone(),
                            args: args.clone(),
                            result: Some(result.clone()),
                        });
                        i += 2;
                        continue;
                    }
                }

                let handler = agent
                    .tools
                    .iter()
                    .find(|t| &t.name == name)
                    .and_then(|t| t.handler.clone());

                let result = match handler {
                    Some(handler) => {
                        match handler(args.clone(), Credentials::default(), ToolContext::default())
                            .await
                        {
                            Ok(value) => Some(value),
                            Err(e) => Some(Value::String(format!("Error: {e}"))),
                        }
                    }
                    None => None,
                };

                tool_calls.push(ToolCallRecord {
                    name: name.clone(),
                    args: args.clone(),
                    result,
                });
                i += 1;
            }
            ScriptedEvent::ToolResult { name, result } => {
                // A `ToolResult` not already consumed by the preceding `ToolCall` branch above
                // (i.e. one with no matching prior call in the script).
                tool_calls.push(ToolCallRecord {
                    name: name.clone(),
                    args: Value::Null,
                    result: Some(result.clone()),
                });
                i += 1;
            }
            ScriptedEvent::Done {
                output: done_output,
            } => {
                output = done_output.clone();
                i += 1;
            }
            ScriptedEvent::Error { message } => {
                output = Value::String(message.clone());
                "FAILED".clone_into(&mut status);
                error = Some(message.clone());
                i += 1;
            }
        }
    }

    AgentResult {
        execution_id: "mock".to_owned(),
        output,
        status,
        error,
        tool_calls,
    }
}

/// Start a fluent assertion chain over `result` -- see [`Expect`]'s methods. Each assertion
/// panics with a clear message on failure (an ordinary Rust `#[test]` failure, not a special
/// error type), and returns `&Self` so calls chain: `expect(&result).completed().used_tool("x")`.
#[must_use]
pub fn expect(result: &AgentResult) -> Expect<'_> {
    Expect { result }
}

/// Fluent assertions over an [`AgentResult`] -- see [`expect`]. Works identically whether
/// `result` came from [`mock_run`] or a live `AgentRuntime::run`/`AgentHandle::join` call, since
/// both produce the same [`AgentResult`] type.
pub struct Expect<'a> {
    result: &'a AgentResult,
}

// Every method here is meant to be called either as the final link in a chain (where the
// returned `&Self` is legitimately discarded -- the assertion already ran via `assert!`) or
// chained further; unlike an ordinary builder, dropping the return value is never a mistake.
#[expect(clippy::must_use_candidate)]
impl Expect<'_> {
    /// Assert the execution completed successfully.
    ///
    /// # Panics
    ///
    /// Panics if the execution didn't complete successfully.
    pub fn completed(&self) -> &Self {
        assert!(
            self.result.is_success(),
            "expected execution to complete successfully, but status was {:?} (error: {:?})",
            self.result.status,
            self.result.error
        );
        self
    }

    /// Assert the execution failed (failed, terminated, or timed out).
    ///
    /// # Panics
    ///
    /// Panics if the execution didn't fail.
    pub fn failed(&self) -> &Self {
        assert!(
            self.result.is_failed(),
            "expected execution to fail, but status was {:?}",
            self.result.status
        );
        self
    }

    /// Assert no error is recorded on the result.
    ///
    /// # Panics
    ///
    /// Panics if an error is recorded.
    pub fn no_errors(&self) -> &Self {
        assert!(
            self.result.error.is_none(),
            "expected no error, but found: {:?}",
            self.result.error
        );
        self
    }

    /// Assert `tool_name` was called at least once.
    ///
    /// # Panics
    ///
    /// Panics if `tool_name` was never called.
    pub fn used_tool(&self, tool_name: &str) -> &Self {
        assert!(
            self.result.tool_calls.iter().any(|c| c.name == tool_name),
            "expected tool {tool_name:?} to have been called, but it wasn't. Tools called: {:?}",
            self.result
                .tool_calls
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        );
        self
    }

    /// Assert `tool_name` was called at least once with exactly `args`.
    ///
    /// # Panics
    ///
    /// Panics if `tool_name` was never called with exactly `args`.
    pub fn used_tool_with_args(&self, tool_name: &str, args: &Value) -> &Self {
        assert!(
            self.result
                .tool_calls
                .iter()
                .any(|c| c.name == tool_name && &c.args == args),
            "expected tool {tool_name:?} to have been called with args {args:?}, but it wasn't. \
             Actual calls to {tool_name:?}: {:?}",
            self.result
                .tool_calls
                .iter()
                .filter(|c| c.name == tool_name)
                .map(|c| &c.args)
                .collect::<Vec<_>>()
        );
        self
    }

    /// Assert `tool_name` was never called.
    ///
    /// # Panics
    ///
    /// Panics if `tool_name` was called.
    pub fn did_not_use_tool(&self, tool_name: &str) -> &Self {
        assert!(
            !self.result.tool_calls.iter().any(|c| c.name == tool_name),
            "expected tool {tool_name:?} to never have been called, but it was"
        );
        self
    }

    /// Assert the final output, converted to a string (its literal text if it's already a JSON
    /// string, or its serialized JSON otherwise), contains `needle`.
    ///
    /// # Panics
    ///
    /// Panics if the output doesn't contain `needle`.
    pub fn output_contains(&self, needle: &str) -> &Self {
        let text = match &self.result.output {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        assert!(
            text.contains(needle),
            "expected output to contain {needle:?}, but got: {text:?}"
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::tool::ToolDef;

    fn agent_with_echo_tool() -> AgentDef {
        AgentDef::new("test_agent")
            .unwrap()
            .with_tool(ToolDef::function::<Value, _, _>(
                "echo",
                "echoes its input",
                serde_json::json!({"type": "object"}),
                |args: Value| async move { Ok(args) },
            ))
    }

    #[tokio::test]
    async fn test_mock_run_auto_executes_a_real_tool_handler() {
        let agent = agent_with_echo_tool();
        let script = vec![
            ScriptedEvent::ToolCall {
                name: "echo".to_owned(),
                args: serde_json::json!({"text": "hi"}),
            },
            ScriptedEvent::Done {
                output: serde_json::json!("done"),
            },
        ];

        let result = mock_run(&agent, "say hi", script).await;

        assert!(result.is_success());
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "echo");
        assert_eq!(
            result.tool_calls[0].result,
            Some(serde_json::json!({"text": "hi"}))
        );
    }

    #[tokio::test]
    async fn test_mock_run_explicit_tool_result_takes_precedence_over_the_real_handler() {
        let agent = agent_with_echo_tool();
        let script = vec![
            ScriptedEvent::ToolCall {
                name: "echo".to_owned(),
                args: serde_json::json!({"text": "hi"}),
            },
            ScriptedEvent::ToolResult {
                name: "echo".to_owned(),
                result: serde_json::json!("scripted result, not the real echo"),
            },
            ScriptedEvent::Done {
                output: serde_json::json!("done"),
            },
        ];

        let result = mock_run(&agent, "say hi", script).await;

        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(
            result.tool_calls[0].result,
            Some(serde_json::json!("scripted result, not the real echo"))
        );
    }

    #[tokio::test]
    async fn test_mock_run_records_a_tool_call_with_no_handler_as_no_result() {
        let agent = AgentDef::new("test_agent").unwrap();
        let script = vec![ScriptedEvent::ToolCall {
            name: "nonexistent".to_owned(),
            args: serde_json::json!({}),
        }];

        let result = mock_run(&agent, "do something", script).await;

        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].result, None);
    }

    #[tokio::test]
    async fn test_mock_run_error_event_produces_a_failed_result() {
        let agent = AgentDef::new("test_agent").unwrap();
        let script = vec![ScriptedEvent::Error {
            message: "something broke".to_owned(),
        }];

        let result = mock_run(&agent, "do something", script).await;

        assert!(result.is_failed());
        assert_eq!(result.error, Some("something broke".to_owned()));
    }

    #[tokio::test]
    async fn test_expect_fluent_assertions_on_a_successful_mock_run() {
        let agent = agent_with_echo_tool();
        let script = vec![
            ScriptedEvent::ToolCall {
                name: "echo".to_owned(),
                args: serde_json::json!({"text": "hi"}),
            },
            ScriptedEvent::Done {
                output: serde_json::json!("all done"),
            },
        ];

        let result = mock_run(&agent, "say hi", script).await;

        expect(&result)
            .completed()
            .no_errors()
            .used_tool("echo")
            .used_tool_with_args("echo", &serde_json::json!({"text": "hi"}))
            .did_not_use_tool("some_other_tool")
            .output_contains("all done");
    }

    #[tokio::test]
    #[should_panic(expected = "expected execution to complete successfully")]
    async fn test_expect_completed_panics_on_a_failed_result() {
        let agent = AgentDef::new("test_agent").unwrap();
        let result = mock_run(
            &agent,
            "do something",
            vec![ScriptedEvent::Error {
                message: "broke".to_owned(),
            }],
        )
        .await;

        expect(&result).completed();
    }

    #[tokio::test]
    #[should_panic(expected = "expected tool")]
    async fn test_expect_used_tool_panics_when_the_tool_was_never_called() {
        let agent = AgentDef::new("test_agent").unwrap();
        let result = mock_run(
            &agent,
            "do something",
            vec![ScriptedEvent::Done {
                output: serde_json::json!("ok"),
            }],
        )
        .await;

        expect(&result).used_tool("echo");
    }
}
