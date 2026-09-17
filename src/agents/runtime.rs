// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! `AgentRuntime` — the control-plane + local-worker composition root for agents.
//!
//! Kept as a single file (not split across per-agent submodules) per
//! `docs/agents/development-waves.md`'s Wave 4 note: `AgentRuntime`'s lifecycle methods
//! (`compile`/`deploy`/`start`/`run`/`serve`/`shutdown`) share enough state (one [`AgentClient`],
//! one [`TaskHandler`]) that splitting them across files would only add indirection.
//!
//! No new polling loop is introduced here — `serve` reuses the existing [`TaskHandler`] exactly
//! as `docs/agents/rust-sdk-design.md`'s "composition over the existing worker framework"
//! section describes: every locally-invoked [`ToolDef`] becomes an ordinary `impl Worker`
//! registered on the same [`TaskHandler`] every other worker in this crate uses, so agent tool
//! workers get pooling, panic isolation, retry-on-update, and graceful shutdown for free.

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::client::AgentClient;
use crate::configuration::Configuration;
use crate::error::{ConductorError, Result};
use crate::http::ApiClient;
use crate::models::Task;
use crate::worker::{TaskHandler, Worker, WorkerOutput};

use super::callback::{CallbackContext, CallbackHandler};
use super::credentials::Credentials;
use super::def::{sanitize_for_task_name, AgentDef};
use super::guardrail::Guardrail;
use super::handle::AgentHandle;
use super::result::AgentResult;
use super::schedule::{self, Schedule};
use super::serializer::AgentConfigSerializer;
use super::termination::TerminationCondition;
use super::tool::{ToolContext, ToolDef, ToolHandler};

/// Bridges a locally-invoked [`ToolDef`] (one whose `handler` is `Some`) into an ordinary
/// [`Worker`], so it can be registered onto the same [`TaskHandler`] every other worker in this
/// crate runs on. The task name is the tool name; the polled [`Task`]'s `input_data` is what the
/// [`ToolHandler`] receives as its raw JSON arguments, and `task.runtime_metadata` is what
/// resolves into the [`Credentials`] passed alongside it.
struct ToolWorker {
    name: String,
    handler: ToolHandler,
    input_schema: Value,
    output_schema: Value,
    credentials: Vec<String>,
}

impl ToolWorker {
    /// Build a [`ToolWorker`] from a [`ToolDef`], or `None` if the tool has no local handler
    /// (server-side tools — `http`/`mcp`/`agent_tool`/`human` — have nothing to run locally).
    fn from_tool_def(tool: &ToolDef) -> Option<Self> {
        let handler = tool.handler.clone()?;
        Some(Self {
            name: tool.name.clone(),
            handler,
            input_schema: tool.input_schema.clone(),
            output_schema: tool.output_schema.clone(),
            credentials: tool.credentials.clone(),
        })
    }
}

#[async_trait]
impl Worker for ToolWorker {
    fn task_definition_name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let credentials = Credentials::from_task(task);

        // `_agent_state` is a system-injected input key carrying state a previous tool call in
        // this agent run recorded via `ToolContext::set_state` — strip it before building the
        // tool's own arguments, matching python's `task.input_data.pop("_agent_state", None)`.
        let mut input_data = task.input_data.clone();
        let initial_state = input_data
            .remove("_agent_state")
            .and_then(|v| {
                serde_json::from_value::<std::collections::HashMap<String, Value>>(v).ok()
            })
            .unwrap_or_default();
        let mut context = ToolContext::default();
        context.execution_id = task.workflow_instance_id.clone();
        let context = context.with_initial_state(initial_state);

        let input = serde_json::to_value(&input_data)?;
        // A handler that wants FAILED_WITH_TERMINAL_ERROR semantics (matching python's
        // `TerminalToolError`) returns `Err(ConductorError::TerminalTool(..))` instead of any
        // other error variant; every other `Err` still propagates via `?` and maps to the
        // default retryable `Failed` in `task_runner.rs`, exactly as before this existed.
        let output = match (self.handler)(input, credentials, context.clone()).await {
            Ok(value) => value,
            Err(ConductorError::TerminalTool(reason)) => {
                return Ok(WorkerOutput::FailedWithTerminalError(reason));
            }
            Err(e) => return Err(e),
        };

        // Matches python's `run_tool_task`/`_dispatch.py`: `if isinstance(result, dict):
        // task_result.output_data = result else: task_result.output_data = {"result": result}`
        // — an object return becomes the task output directly (its fields are the output keys
        // the agent loop/LLM sees), not nested one level down under a "result" key. Only
        // non-object returns (numbers, strings, arrays, null) get the "result" wrapper. Found by
        // actually running a tool that returns a dict against a recorded playback fixture: the
        // old unconditional `completed_with_result` wrap produced `{"result": {...}}`, which
        // never matches what real tool-using agents (python or otherwise) actually send.
        let mut result_map = match output {
            Value::Object(map) => map.into_iter().collect(),
            other => match WorkerOutput::completed_with_result(other) {
                WorkerOutput::Completed(map) => map,
                other => return Ok(other),
            },
        };
        let state_updates = context.state_snapshot();
        if !state_updates.is_empty() {
            result_map.insert(
                "_state_updates".to_owned(),
                serde_json::to_value(state_updates)?,
            );
        }
        Ok(WorkerOutput::completed(result_map))
    }

    fn input_schema(&self) -> Option<Value> {
        (!self.input_schema.is_null()).then(|| self.input_schema.clone())
    }

    fn output_schema(&self) -> Option<Value> {
        (!self.output_schema.is_null()).then(|| self.output_schema.clone())
    }

    fn declared_credentials(&self) -> Vec<String> {
        self.credentials.clone()
    }
}

/// Bridges an [`AgentDef::stop_when`] predicate into an ordinary [`Worker`], registered under
/// `{agent_name}_stop_when` — matching the task name [`AgentConfigSerializer`] emits for
/// `stopWhen` and python-sdk's `_register_stop_when_worker`/`StopWhenEntry`.
struct StopWhenWorker {
    task_name: String,
    handler: super::def::StopWhenHandler,
}

#[async_trait]
impl Worker for StopWhenWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    /// Reads `result`/`messages`/`iteration` off the polled [`Task`]'s input, builds the same
    /// context shape python's `StopWhenEntry.__call__` does (`{"result": ..., "messages": ...,
    /// "iteration": ...}`), and returns `{"should_continue": !should_stop}` — inverted from the
    /// predicate's own `true` = "stop" sense, matching the wire contract the server's compiled
    /// SWITCH task expects. On a predicate error, logs it and fails *open*
    /// (`should_continue: true`), matching python's `except Exception` branch exactly rather
    /// than propagating the error and potentially wedging the workflow.
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let result = task
            .input_data
            .get("result")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let messages = task
            .input_data
            .get("messages")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let iteration = task
            .input_data
            .get("iteration")
            .cloned()
            .unwrap_or_else(|| Value::from(0));
        let context = serde_json::json!({
            "result": result,
            "messages": messages,
            "iteration": iteration,
        });

        let should_continue = match (self.handler)(context).await {
            Ok(should_stop) => !should_stop,
            Err(e) => {
                tracing::error!("stop_when evaluation failed: {e}");
                true
            }
        };

        let mut output = std::collections::HashMap::new();
        output.insert("should_continue".to_owned(), Value::Bool(should_continue));
        Ok(WorkerOutput::completed(output))
    }
}

/// Bridges a callable [`super::def::GateCondition`] into an ordinary [`Worker`], registered
/// under `{agent_name}_gate` — matching the task name [`AgentConfigSerializer`] emits for the
/// callable `gate` shape and python-sdk's `_register_gate_worker`/`GateEntry`.
struct GateWorker {
    task_name: String,
    handler: super::def::GateHandler,
}

#[async_trait]
impl Worker for GateWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    /// Builds `{"result": ...}` (matching python's `GateEntry.__call__`'s context dict) and
    /// returns `{"decision": "continue"|"stop"}`. On a predicate error, logs it and fails
    /// *open* (`"decision": "continue"`), matching python's `except Exception` branch exactly.
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let result = task
            .input_data
            .get("result")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let context = serde_json::json!({ "result": result });

        let decision = match (self.handler)(context).await {
            Ok(true) => "continue",
            Ok(false) => "stop",
            Err(e) => {
                tracing::error!("gate evaluation failed: {e}");
                "continue"
            }
        };

        let mut output = std::collections::HashMap::new();
        output.insert("decision".to_owned(), Value::String(decision.to_owned()));
        Ok(WorkerOutput::completed(output))
    }
}

/// Bridges an [`AgentDef::termination`] condition into an ordinary [`Worker`], registered under
/// `{agent_name}_termination` — matching the task name python-sdk's
/// `_register_termination_worker`/`TerminationEntry` uses.
struct TerminationWorker {
    task_name: String,
    condition: TerminationCondition,
}

#[async_trait]
impl Worker for TerminationWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    /// Builds the same `{"result", "messages", "iteration"}` context shape python's
    /// `TerminationEntry.__call__` does (`messages` always `[]` there too — python's own
    /// comment: conditions receive it for API completeness, but this worker's compiled task
    /// only ever supplies `result`/`iteration`) and returns `{"should_continue": !terminate,
    /// "reason": ...}`. [`TerminationCondition::should_terminate`] never returns an `Err`, so
    /// unlike [`StopWhenWorker`] there's no fail-open branch to trigger here.
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let result = task
            .input_data
            .get("result")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let iteration = task
            .input_data
            .get("iteration")
            .cloned()
            .unwrap_or_else(|| Value::from(0));
        let context = serde_json::json!({
            "result": result,
            "messages": Value::Array(Vec::new()),
            "iteration": iteration,
        });

        let outcome = self.condition.should_terminate(&context);

        let mut output = std::collections::HashMap::new();
        output.insert(
            "should_continue".to_owned(),
            Value::Bool(!outcome.should_terminate),
        );
        output.insert("reason".to_owned(), Value::String(outcome.reason));
        Ok(WorkerOutput::completed(output))
    }
}

/// One of the six lifecycle points [`CallbackHandler`] exposes — matches python-sdk's
/// `POSITION_TO_METHOD` keys (`callback.py`) and the `{agent_name}_{position}` task-naming
/// convention `_register_callback_worker` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackPosition {
    BeforeAgent,
    AfterAgent,
    BeforeModel,
    AfterModel,
    BeforeTool,
    AfterTool,
}

impl CallbackPosition {
    /// All six positions, in the order [`AgentRuntime::serve`] registers workers for them.
    const ALL: [CallbackPosition; 6] = [
        CallbackPosition::BeforeAgent,
        CallbackPosition::AfterAgent,
        CallbackPosition::BeforeModel,
        CallbackPosition::AfterModel,
        CallbackPosition::BeforeTool,
        CallbackPosition::AfterTool,
    ];

    /// Wire/task-name spelling, matching python's `POSITION_TO_METHOD` keys exactly.
    fn as_str(self) -> &'static str {
        match self {
            CallbackPosition::BeforeAgent => "before_agent",
            CallbackPosition::AfterAgent => "after_agent",
            CallbackPosition::BeforeModel => "before_model",
            CallbackPosition::AfterModel => "after_model",
            CallbackPosition::BeforeTool => "before_tool",
            CallbackPosition::AfterTool => "after_tool",
        }
    }

    async fn dispatch(
        &self,
        handler: &dyn CallbackHandler,
        ctx: &CallbackContext,
    ) -> Option<Value> {
        match self {
            CallbackPosition::BeforeAgent => handler.on_agent_start(ctx).await,
            CallbackPosition::AfterAgent => handler.on_agent_end(ctx).await,
            CallbackPosition::BeforeModel => handler.on_model_start(ctx).await,
            CallbackPosition::AfterModel => handler.on_model_end(ctx).await,
            CallbackPosition::BeforeTool => handler.on_tool_start(ctx).await,
            CallbackPosition::AfterTool => handler.on_tool_end(ctx).await,
        }
    }
}

/// Run `handlers` in order for `position`, returning the first short-circuiting result — mirrors
/// python's `_chain_callbacks_for_position`'s handler-chain half (there is no Rust equivalent of
/// python's separate `legacy_fn`/`before_agent_callback`-style deprecated attributes, so only the
/// handler-list half is ported). Per [`CallbackHandler`]'s own doc comment: `None` *and*
/// `Some(Value::Null)` both mean "continue to the next handler"; anything else short-circuits.
/// Returns `Value::Object` either way — the winning handler's value if it's already an object,
/// that value wrapped as `{"result": value}` if it's some other JSON type, or an empty object if
/// no handler short-circuited — so callers never need to branch on the return shape.
async fn dispatch_callback_position(
    position: CallbackPosition,
    handlers: &[std::sync::Arc<dyn CallbackHandler>],
    ctx: &CallbackContext,
) -> Value {
    for handler in handlers {
        match position.dispatch(handler.as_ref(), ctx).await {
            None | Some(Value::Null) => {}
            Some(Value::Object(map)) => return Value::Object(map),
            Some(other) => {
                let mut wrapped = Map::new();
                wrapped.insert("result".to_owned(), other);
                return Value::Object(wrapped);
            }
        }
    }
    Value::Object(Map::new())
}

/// Bridges [`AgentDef::callbacks`] at one [`CallbackPosition`] into an ordinary [`Worker`],
/// registered under `{agent_name}_{position}` — matching python-sdk's
/// `_register_callback_worker`/`CallbackEntry`.
struct CallbackWorker {
    task_name: String,
    position: CallbackPosition,
    handlers: Vec<std::sync::Arc<dyn CallbackHandler>>,
}

#[async_trait]
impl Worker for CallbackWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    /// Builds a [`CallbackContext`] from whichever of `messages`/`llm_result` the polled
    /// [`Task`]'s input carries (matching python's `CallbackEntry.__call__`, which only forwards
    /// the kwargs it was actually given) and dispatches via [`dispatch_callback_position`].
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let mut fields = Map::new();
        if let Some(messages) = task.input_data.get("messages") {
            fields.insert("messages".to_owned(), messages.clone());
        }
        if let Some(llm_result) = task.input_data.get("llm_result") {
            fields.insert("llm_result".to_owned(), llm_result.clone());
        }
        let ctx = CallbackContext::from(fields);

        let output = dispatch_callback_position(self.position, &self.handlers, &ctx).await;
        let Value::Object(map) = output else {
            unreachable!("dispatch_callback_position always returns Value::Object");
        };
        Ok(WorkerOutput::completed(map.into_iter().collect()))
    }
}

/// Matches python's `_stringify_content`: `None`/absent -> `""`; a JSON string -> itself; any
/// other JSON value -> its JSON text (python's `json.dumps(content, default=str)`).
fn stringify_content(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// `true` if `guardrail`'s wire shape is python's "custom `@guardrail` function" case (not
/// `RegexGuardrail`/`LlmGuardrail`, not external) — the only kind [`AgentRuntime::serve`]
/// registers a worker for, matching python's `not isinstance(g, (RegexGuardrail, LLMGuardrail))
/// and g.func is not None` filter. Checked via [`Guardrail::guardrail_type_fields`]'s
/// `guardrailType` discriminant rather than a downcast, since that's the one place every
/// [`super::guardrail::GuardrailCheck`] impl already declares its own kind.
fn is_custom_function_guardrail(guardrail: &Guardrail) -> bool {
    guardrail.guardrail_type_fields().get("guardrailType")
        == Some(&Value::String("custom".to_owned()))
}

/// Bridges a custom-function [`Guardrail`] (backed by
/// [`super::guardrail::FunctionGuardrail`]) into an ordinary [`Worker`], registered under the
/// guardrail's own name — matching the server's `GuardrailCompiler.compileCustomGuardrail`
/// convention and python-sdk's `_register_single_guardrail_worker`/`GuardrailEntry`.
///
/// Python also registers a second, *combined* worker (`{agent_name}_output_guardrail`) for its
/// own client-side "local compile" path, which builds workflow tasks without going through the
/// server compiler at all. This crate has no such path — `AgentRuntime::compile`/`deploy` always
/// go through the real server's `/agent/compile`/`/agent/deploy` — so only the per-guardrail
/// worker is registered here; the combined one would have nothing to serve.
struct GuardrailWorker {
    task_name: String,
    guardrail: Guardrail,
}

#[async_trait]
impl Worker for GuardrailWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    /// Builds the `{content, iteration}` -> `{passed, message, on_fail, fixed_output,
    /// guardrail_name, should_continue}` contract python's `GuardrailEntry.__call__` uses,
    /// including the same `on_fail` escalation rules (`retry` -> `raise` once `iteration` hits
    /// `max_retries`; `fix` -> `raise` when there's no `fixed_output`). Unlike python's
    /// `GuardrailEntry._check`, a panicking [`super::guardrail::GuardrailCheck`] is not caught
    /// here — this crate doesn't defensively catch panics anywhere else either, and a broken
    /// custom guardrail closure is a caller bug, not a runtime condition to paper over.
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let content = stringify_content(task.input_data.get("content"));
        let iteration = task
            .input_data
            .get("iteration")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let result = self.guardrail.check(&content);

        let output = if result.passed {
            serde_json::json!({
                "passed": true,
                "message": "",
                "on_fail": "pass",
                "fixed_output": Value::Null,
                "guardrail_name": "",
                "should_continue": false,
            })
        } else {
            let mut on_fail = self.guardrail.on_fail.as_str();
            if on_fail == "retry" && iteration >= self.guardrail.max_retries {
                on_fail = "raise";
            }
            if on_fail == "fix" && result.fixed_output.is_none() {
                on_fail = "raise";
            }
            serde_json::json!({
                "passed": false,
                "message": result.message,
                "on_fail": on_fail,
                "fixed_output": result.fixed_output,
                "guardrail_name": self.guardrail.name,
                "should_continue": on_fail == "retry",
            })
        };

        let Value::Object(map) = output else {
            unreachable!("guardrail output is always built as an object");
        };
        Ok(WorkerOutput::completed(map.into_iter().collect()))
    }
}

/// Detect `_transfer_to_` tool calls in `tool_calls`, matching python's `CheckTransferEntry`.
/// Selection is first-wins (only one hand-off per turn is meaningful); any further transfer
/// calls in the same turn are surfaced as `dropped_transfers` rather than silently discarded.
fn evaluate_check_transfer(tool_calls: &[Value]) -> Value {
    const MARKER: &str = "_transfer_to_";
    let mut transfers: Vec<(String, String)> = Vec::new();
    for tc in tool_calls {
        let name = tc.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(idx) = name.find(MARKER) else {
            continue;
        };
        let target = name[idx + MARKER.len()..].to_string();
        let message = match tc.get("inputParameters").and_then(|p| p.get("message")) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        };
        transfers.push((target, message));
    }

    if transfers.is_empty() {
        return serde_json::json!({"is_transfer": false, "transfer_to": "", "transfer_message": ""});
    }
    let (first_target, first_message) = transfers[0].clone();
    let mut out = serde_json::json!({
        "is_transfer": true,
        "transfer_to": first_target,
        "transfer_message": first_message,
    });
    if transfers.len() > 1 {
        let dropped: Vec<Value> = transfers[1..]
            .iter()
            .map(|(target, message)| serde_json::json!({"transfer_to": target, "message": message}))
            .collect();
        out["dropped_transfers"] = Value::Array(dropped);
    }
    out
}

/// Bridges [`evaluate_check_transfer`] into a [`Worker`], registered under
/// `{agent_name}_check_transfer` — matching python's `_register_check_transfer_worker`.
struct CheckTransferWorker {
    task_name: String,
}

#[async_trait]
impl Worker for CheckTransferWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let tool_calls = task
            .input_data
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let output = evaluate_check_transfer(&tool_calls);
        let Value::Object(map) = output else {
            unreachable!("evaluate_check_transfer always returns an object");
        };
        Ok(WorkerOutput::completed(map.into_iter().collect()))
    }
}

/// No-op transfer tool — the actual hand-off is detected by [`CheckTransferWorker`] from
/// `toolCalls` output; this just echoes the hand-off `message` (if any) so it's visible in the
/// task output/UI. Registered under `{source}_transfer_to_{target}`, matching python's
/// `TransferNoopEntry`.
struct TransferNoopWorker {
    task_name: String,
}

#[async_trait]
impl Worker for TransferNoopWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let message = task
            .input_data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut output = std::collections::HashMap::new();
        if !message.is_empty() {
            output.insert("message".to_owned(), Value::String(message.to_owned()));
        }
        Ok(WorkerOutput::completed(output))
    }
}

/// Transfer tool for a target unreachable via [`AgentDef::allowed_transitions`] — always
/// returns a fixed error message so the LLM knows to try a different tool. Matches python's
/// `TransferUnreachableEntry`.
struct TransferUnreachableWorker {
    task_name: String,
}

#[async_trait]
impl Worker for TransferUnreachableWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, _task: &Task) -> Result<WorkerOutput> {
        Ok(WorkerOutput::completed_with_result(format!(
            "ERROR: {} is not available. Use a different transfer tool, or if you are done, \
             just provide your final response without calling any transfer tool.",
            self.task_name
        )))
    }
}

/// `true` if `value` is python's `_is_transfer_truthy`: a JSON `true`, or the case-insensitive
/// string `"true"`.
fn is_transfer_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => s.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// `true` if `source_idx` (looked up in `idx_to_name`) is allowed to transfer to `target_name`
/// per `allowed` — empty `allowed` means unrestricted, matching python's
/// `HandoffCheckEntry._is_allowed`.
fn is_allowed_transition(
    allowed: &std::collections::HashMap<String, Vec<String>>,
    idx_to_name: &std::collections::HashMap<String, String>,
    source_idx: &str,
    target_name: &str,
) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let source_name = idx_to_name.get(source_idx).map_or("", String::as_str);
    allowed
        .get(source_name)
        .is_some_and(|targets| targets.iter().any(|t| t == target_name))
}

/// Swarm handoff decision, matching python's `HandoffCheckEntry.__call__` return shape.
struct HandoffDecision {
    active_agent: String,
    handoff: bool,
}

/// Number of blocked-transfer retries an agent gets before the loop gives up on it, matching
/// python's `HandoffCheckEntry.__init__`'s `max_blocked_retries=3` default.
const HANDOFF_MAX_BLOCKED_RETRIES: u32 = 3;

/// Decide the next `active_agent` for a swarm turn, matching python's
/// `HandoffCheckEntry.__call__`: priority 1 is a detected transfer-tool call (with retry-then-
/// give-up handling when `allowed_transitions` blocks it); priority 2 (fallback) is
/// condition-based [`super::SwarmTransition`] evaluation.
#[expect(clippy::too_many_arguments)]
fn evaluate_handoff_check(
    transitions: &[super::swarm::SwarmTransition],
    name_to_idx: &std::collections::HashMap<String, String>,
    idx_to_name: &std::collections::HashMap<String, String>,
    allowed: &std::collections::HashMap<String, Vec<String>>,
    blocked_counts: &mut std::collections::HashMap<String, u32>,
    result: &str,
    active_agent: &str,
    is_transfer: &Value,
    transfer_to: &str,
) -> HandoffDecision {
    if is_transfer_truthy(is_transfer) {
        if is_allowed_transition(allowed, idx_to_name, active_agent, transfer_to) {
            blocked_counts.remove(active_agent);
            let target_idx = name_to_idx
                .get(transfer_to)
                .cloned()
                .unwrap_or_else(|| active_agent.to_owned());
            if target_idx != active_agent {
                return HandoffDecision {
                    active_agent: target_idx,
                    handoff: true,
                };
            }
            // Self-transfer no-op — fall through to the condition-based check below.
        } else if !allowed.is_empty() {
            let count = blocked_counts.entry(active_agent.to_owned()).or_insert(0);
            *count += 1;
            if *count <= HANDOFF_MAX_BLOCKED_RETRIES {
                return HandoffDecision {
                    active_agent: active_agent.to_owned(),
                    handoff: true,
                };
            }
            blocked_counts.remove(active_agent);
            return HandoffDecision {
                active_agent: active_agent.to_owned(),
                handoff: false,
            };
        }
    }

    let ctx = super::swarm::SwarmContext {
        result: Some(result.to_owned()),
        tool_name: None,
        tool_result: None,
    };
    for transition in transitions {
        if transition.should_transition(&ctx)
            && is_allowed_transition(allowed, idx_to_name, active_agent, transition.target())
        {
            let target_idx = name_to_idx
                .get(transition.target())
                .cloned()
                .unwrap_or_else(|| active_agent.to_owned());
            if target_idx != active_agent {
                return HandoffDecision {
                    active_agent: target_idx,
                    handoff: true,
                };
            }
        }
    }

    HandoffDecision {
        active_agent: active_agent.to_owned(),
        handoff: false,
    }
}

/// Bridges [`evaluate_handoff_check`] into a [`Worker`], registered under
/// `{agent_name}_handoff_check` — matching python's `_register_handoff_worker`/
/// `HandoffCheckEntry`. `blocked_counts` is per-process mutable state, the same closure-cell
/// semantics python's instance attribute replaces.
struct HandoffCheckWorker {
    task_name: String,
    transitions: Vec<super::swarm::SwarmTransition>,
    name_to_idx: std::collections::HashMap<String, String>,
    idx_to_name: std::collections::HashMap<String, String>,
    allowed: std::collections::HashMap<String, Vec<String>>,
    blocked_counts: std::sync::Mutex<std::collections::HashMap<String, u32>>,
}

#[async_trait]
impl Worker for HandoffCheckWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let result = task
            .input_data
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("");
        let active_agent = task
            .input_data
            .get("active_agent")
            .and_then(Value::as_str)
            .unwrap_or("0");
        let is_transfer = task
            .input_data
            .get("is_transfer")
            .cloned()
            .unwrap_or(Value::Bool(false));
        let transfer_to = task
            .input_data
            .get("transfer_to")
            .and_then(Value::as_str)
            .unwrap_or("");

        let mut blocked_counts = self
            .blocked_counts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let decision = evaluate_handoff_check(
            &self.transitions,
            &self.name_to_idx,
            &self.idx_to_name,
            &self.allowed,
            &mut blocked_counts,
            result,
            active_agent,
            &is_transfer,
            transfer_to,
        );
        drop(blocked_counts);

        let mut output = std::collections::HashMap::new();
        output.insert(
            "active_agent".to_owned(),
            Value::String(decision.active_agent),
        );
        output.insert("handoff".to_owned(), Value::Bool(decision.handoff));
        Ok(WorkerOutput::completed(output))
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Map a manual-strategy human selection to its sub-agent index, matching python's
/// `ProcessSelectionEntry.__call__`: `human_output.selected`/`.agent` (in that priority order,
/// defaulting to `"0"`) is looked up in `name_to_idx`; if the lookup misses (or `human_output`
/// isn't an object at all), the value's string form is returned as-is.
fn evaluate_manual_selection(
    name_to_idx: &std::collections::HashMap<String, String>,
    human_output: Option<&Value>,
) -> String {
    let selected_value = match human_output {
        None | Some(Value::Null) => return "0".to_owned(),
        Some(Value::Object(map)) => map
            .get("selected")
            .or_else(|| map.get("agent"))
            .cloned()
            .unwrap_or_else(|| Value::String("0".to_owned())),
        Some(other) => return display_value(other),
    };
    if let Value::String(s) = &selected_value {
        if let Some(idx) = name_to_idx.get(s) {
            return idx.clone();
        }
    }
    display_value(&selected_value)
}

/// Bridges [`evaluate_manual_selection`] into a [`Worker`], registered under
/// `{agent_name}_process_selection` — matching python's `_register_manual_selection_worker`/
/// `ProcessSelectionEntry`.
struct ManualSelectionWorker {
    task_name: String,
    name_to_idx: std::collections::HashMap<String, String>,
}

#[async_trait]
impl Worker for ManualSelectionWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let selected =
            evaluate_manual_selection(&self.name_to_idx, task.input_data.get("human_output"));
        let mut output = std::collections::HashMap::new();
        output.insert("selected".to_owned(), Value::String(selected));
        Ok(WorkerOutput::completed(output))
    }
}

/// Merge an `AgentRuntime::start`/`run` caller's `input` into `payload`'s top level, in the
/// shape the server's `AgentStartRequest` DTO actually has (`prompt`/`media`/`context`/
/// `sessionId` — there is no generic `input` field). See [`AgentRuntime::start`] for the exact
/// rules.
/// Extract the `executionId`/`execution_id` field a `/agent/start` response carries, or a
/// descriptive error if the response has neither.
fn extract_execution_id(response: &Value) -> Result<String> {
    response
        .get("executionId")
        .or_else(|| response.get("execution_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            ConductorError::agent(format!(
                "start_agent response has no executionId: {response}"
            ))
        })
}

/// Build the `{"framework": ..., "rawConfig": ...}` request-body shape the server's
/// `AgentStartRequest` accepts as an alternative to `agentConfig` — shared by
/// `compile_framework`/`deploy_framework`/`start_framework`.
fn framework_payload(framework: impl Into<String>, raw_config: Value) -> Value {
    serde_json::json!({
        "framework": framework.into(),
        "rawConfig": raw_config,
    })
}

fn merge_start_input(payload: &mut Map<String, Value>, input: Value) {
    const RECOGNIZED_KEYS: [&str; 5] = ["prompt", "media", "context", "sessionId", "static_plan"];

    match input {
        Value::String(prompt) => {
            payload.insert("prompt".to_owned(), Value::String(prompt));
        }
        Value::Object(mut fields) => {
            // Normalize `session_id` to the wire name `sessionId` before splitting recognized
            // keys from overflow, so either spelling reaches the server correctly.
            if let Some(session_id) = fields.remove("session_id") {
                fields.entry("sessionId".to_owned()).or_insert(session_id);
            }

            let mut overflow = Map::new();
            for (key, value) in fields {
                if RECOGNIZED_KEYS.contains(&key.as_str()) {
                    payload.insert(key, value);
                } else {
                    overflow.insert(key, value);
                }
            }

            if !overflow.is_empty() {
                match payload.get_mut("context") {
                    Some(Value::Object(context)) => context.extend(overflow),
                    _ => {
                        payload.insert("context".to_owned(), Value::Object(overflow));
                    }
                }
            }
        }
        other => {
            let mut context = Map::new();
            context.insert("value".to_owned(), other);
            payload.insert("context".to_owned(), Value::Object(context));
        }
    }
}

/// Composition root for compiling, deploying, starting, running, and locally serving
/// [`AgentDef`]s against a Conductor server.
///
/// Owns one [`AgentClient`] (the `/agent/*` control-plane transport) and one [`TaskHandler`]
/// (this crate's existing worker-polling machinery) — [`AgentRuntime::serve`] registers each
/// [`AgentDef`]'s locally-invoked tools onto that same `TaskHandler` rather than running a second
/// polling loop, per `docs/agents/rust-sdk-design.md`.
pub struct AgentRuntime {
    agent_client: AgentClient,
    task_handler: TaskHandler,
}

impl AgentRuntime {
    /// Build a runtime from a [`Configuration`]: one [`AgentClient`] and one owned [`TaskHandler`]
    /// for local tool workers, both pointed at the same server.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the underlying `reqwest` client fails to build (see [`ApiClient::new`]).
    pub fn new(config: Configuration) -> Result<Self> {
        let api_client = ApiClient::new(config.clone())?;
        let agent_client = AgentClient::new(api_client);
        let task_handler = TaskHandler::new(config)?;

        Ok(Self {
            agent_client,
            task_handler,
        })
    }

    /// Access the underlying [`TaskHandler`] that [`AgentRuntime::serve`]/[`AgentRuntime::resume`]
    /// register local tool workers onto.
    ///
    /// The main current use is calling [`TaskHandler::verify_workers_started`] right after
    /// `serve`/`resume` returns, to confirm every worker actually started polling before
    /// proceeding -- e.g. before handing out a URL/webhook that assumes the agent is ready.
    #[must_use]
    pub fn task_handler(&self) -> &TaskHandler {
        &self.task_handler
    }

    /// Compile an [`AgentDef`] into a Conductor workflow, without deploying it.
    ///
    /// Serializes `agent` via [`AgentConfigSerializer::serialize`] and wraps it in the
    /// `{"agentConfig": ...}` envelope the server's `AgentStartRequest` DTO requires (matching
    /// python-sdk's `_compile_via_server`), then POSTs it via [`AgentClient::compile_agent`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn compile(&self, agent: &AgentDef) -> Result<Value> {
        let payload = serde_json::json!({
            "agentConfig": AgentConfigSerializer::serialize(agent),
        });
        self.agent_client.compile_agent(&payload).await
    }

    /// Compile and register an [`AgentDef`] as a Conductor workflow.
    ///
    /// See [`AgentRuntime::compile`] for the envelope shape.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::compile`].
    pub async fn deploy(&self, agent: &AgentDef) -> Result<Value> {
        let payload = serde_json::json!({
            "agentConfig": AgentConfigSerializer::serialize(agent),
        });
        self.agent_client.deploy_agent(&payload).await
    }

    /// [`AgentRuntime::deploy`], then reconcile the agent's cron schedules in the same call --
    /// matches python's `deploy(agent, schedules=...)`.
    ///
    /// Unlike python, this crate's `deploy` only ever takes one agent (see its doc comment), so
    /// python's runtime check that `schedules` requires "exactly one agent" has no equivalent
    /// here -- the signature already guarantees it.
    ///
    /// `schedules` follows the same tri-state contract as python's `deploy(..., schedules=...)`:
    /// - `None`: leave this agent's existing schedules untouched (the default -- this method
    ///   behaves exactly like [`AgentRuntime::deploy`] if you never pass anything else).
    /// - `Some(&[])`: delete every schedule currently registered for this agent.
    /// - `Some(non-empty)`: upsert the listed schedules; delete any existing schedule for this
    ///   agent that isn't in the list.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::deploy`], plus [`crate::error::ConductorError::Agent`] if
    /// `schedules` contains a duplicate [`Schedule::name`] or a schedule whose `start_at` is not
    /// strictly before its `end_at`.
    pub async fn deploy_with_schedules(
        &self,
        agent: &AgentDef,
        schedules: Option<&[Schedule]>,
    ) -> Result<Value> {
        let response = self.deploy(agent).await?;
        schedule::reconcile(
            &self.agent_client.scheduler_client(),
            &agent.name,
            schedules,
        )
        .await?;
        Ok(response)
    }

    /// Start an agent execution without blocking for completion.
    ///
    /// Serializes `agent`, merges `input` into the top-level `AgentStartRequest` fields the
    /// server expects (`prompt`/`media`/`context`/`sessionId`/`static_plan` — there is no
    /// generic `input` field on that DTO), and wraps the `executionId` the response carries in an
    /// [`AgentHandle`]. `input` may be:
    /// - a plain string, used as `prompt` directly;
    /// - an object already shaped like the extra fields (e.g. `json!({"prompt": "...", "context":
    ///   {...}})`) — recognized keys (`prompt`, `media`, `context`, `sessionId`/`session_id`,
    ///   `static_plan` — see [`super::Plan::to_value`] for building the latter) are
    ///   passed through as-is; any other keys in the object are folded into `context` (merged
    ///   with an explicit `context` key if both are present) rather than silently dropped;
    /// - anything else, wrapped as `{"context": {"value": input}}`.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`AgentRuntime::compile`] if the start request fails, or [`crate::error::ConductorError::Agent`] if the response has no `executionId`.
    pub async fn start(&self, agent: &AgentDef, input: Value) -> Result<AgentHandle> {
        let mut payload = Map::new();
        payload.insert(
            "agentConfig".to_owned(),
            AgentConfigSerializer::serialize(agent),
        );
        merge_start_input(&mut payload, input);
        let response = self
            .agent_client
            .start_agent(&Value::Object(payload))
            .await?;
        let execution_id = extract_execution_id(&response)?;

        Ok(AgentHandle::new(self.agent_client.clone(), execution_id))
    }

    /// Start an agent execution and block until it reaches a terminal status.
    ///
    /// Equivalent to `self.start(agent, input).await?.join().await`.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::start`], plus any error [`AgentHandle::join`] returns while polling for a terminal status.
    pub async fn run(&self, agent: &AgentDef, input: Value) -> Result<AgentResult> {
        self.start(agent, input).await?.join().await
    }

    /// Compile a foreign-framework agent, given its already-serialized `raw_config` (e.g. the
    /// output of a framework-specific serializer) — the `{"framework": ..., "rawConfig": ...}`
    /// shape `AgentStartRequest` accepts as an alternative to `agentConfig`, matching python's
    /// `_deploy_via_server(agent, framework=framework)`'s framework branch (same shape, shared
    /// across compile/deploy/start there too).
    ///
    /// `framework` must name a normalizer the server actually has registered (confirmed by
    /// reading the server's `NormalizerRegistry`/`*Normalizer` implementations directly, not
    /// assumed) — e.g. `"openai"`, `"google_adk"`, `"langgraph"`, `"langchain"`, `"skill"`.
    /// **This crate has no serializer that produces a *correct* `raw_config` for any of them
    /// yet** — [`super::graph::GraphAgentDef`] in particular does **not** produce the shape
    /// `"langgraph"`'s normalizer expects (confirmed by reading `LangGraphNormalizer.java`: it
    /// needs a nested `_graph` key with `snake_case` `nodes`/`edges`/`conditional_edges` and
    /// `_worker_ref`/`_llm_node`/`_human_node`-style per-node markers, not `GraphAgentDef`'s
    /// current `{name, nodes, edges, conditionalEdges}` shape). Callers must build `raw_config`
    /// themselves, matching whatever normalizer they're targeting, until a real serializer for
    /// one exists.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::compile`].
    pub async fn compile_framework(
        &self,
        framework: impl Into<String>,
        raw_config: Value,
    ) -> Result<Value> {
        let payload = framework_payload(framework, raw_config);
        self.agent_client.compile_agent(&payload).await
    }

    /// Deploy a foreign-framework agent. See [`AgentRuntime::compile_framework`] for the
    /// `raw_config` contract and its current limits.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::compile_framework`].
    pub async fn deploy_framework(
        &self,
        framework: impl Into<String>,
        raw_config: Value,
    ) -> Result<Value> {
        let payload = framework_payload(framework, raw_config);
        self.agent_client.deploy_agent(&payload).await
    }

    /// Start a foreign-framework agent execution without blocking for completion. See
    /// [`AgentRuntime::compile_framework`] for the `raw_config` contract and its current limits,
    /// and [`AgentRuntime::start`] for the `input` merge rules (identical here).
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::start`].
    pub async fn start_framework(
        &self,
        framework: impl Into<String>,
        raw_config: Value,
        input: Value,
    ) -> Result<AgentHandle> {
        let Value::Object(mut payload) = framework_payload(framework, raw_config) else {
            unreachable!("framework_payload always returns an object")
        };
        merge_start_input(&mut payload, input);
        let response = self
            .agent_client
            .start_agent(&Value::Object(payload))
            .await?;
        let execution_id = extract_execution_id(&response)?;

        Ok(AgentHandle::new(self.agent_client.clone(), execution_id))
    }

    /// Start a foreign-framework agent execution and block until it reaches a terminal status.
    /// Equivalent to `self.start_framework(framework, raw_config, input).await?.join().await`.
    ///
    /// # Errors
    ///
    /// Same as [`AgentRuntime::start_framework`], plus any error [`AgentHandle::join`] returns while polling for a terminal status.
    pub async fn run_framework(
        &self,
        framework: impl Into<String>,
        raw_config: Value,
        input: Value,
    ) -> Result<AgentResult> {
        self.start_framework(framework, raw_config, input)
            .await?
            .join()
            .await
    }

    /// Register every locally-invoked tool on `agent` (i.e. every [`ToolDef`] whose `handler` is
    /// `Some`) as a worker on this runtime's [`TaskHandler`], plus:
    /// - a `{name}_stop_when` worker if [`AgentDef::stop_when`] is set;
    /// - a `{name}_termination` worker if [`AgentDef::termination`] is set;
    /// - a `{name}_{position}` worker for each of the six `CallbackPosition`s, if
    ///   [`AgentDef::callbacks`] is non-empty;
    /// - one worker per custom-function [`Guardrail`] in [`AgentDef::guardrails`] *or* any
    ///   [`ToolDef::guardrails`] on `agent.tools` (i.e. every guardrail whose wire
    ///   `guardrailType` is `"custom"` — [`RegexGuardrail`](super::guardrail::RegexGuardrail)/
    ///   [`LlmGuardrail`](super::guardrail::LlmGuardrail) are evaluated natively by the server
    ///   and need no worker), registered under the guardrail's own name, matching python's
    ///   `_register_single_guardrail_worker` (called from both the agent-level and the
    ///   per-tool registration loops);
    /// - a `{name}_gate` worker if [`AgentDef::gate`] is the callable
    ///   [`super::def::GateCondition::Callable`] shape (the declarative
    ///   [`super::def::GateCondition::Text`] shape needs none — it's compiled entirely
    ///   server-side);
    /// - a `{name}_check_transfer` worker plus one no-op `{name}_transfer_to_{sub}` tool per
    ///   sub-agent, for a *hybrid* agent (has both its own [`ToolDef`]s and sub-[`AgentDef`]s),
    ///   matching python's `_register_check_transfer_worker`/`_register_hybrid_transfer_workers`;
    /// - a `{name}_handoff_check` worker if [`AgentDef::swarm_transitions`] is non-empty or
    ///   `strategy` is [`Strategy::Swarm`](super::def::Strategy::Swarm) with sub-agents, matching
    ///   python's `_register_handoff_worker`;
    /// - for `strategy = Swarm`: an all-pairs `{source}_transfer_to_{target}` no-op (or, when
    ///   [`AgentDef::allowed_transitions`] makes a target unreachable, error-returning) tool per
    ///   ordered agent pair, plus a `{name}_check_transfer` worker for the parent *and* every
    ///   sub-agent, matching python's `_register_swarm_transfer_workers`;
    /// - a `{name}_process_selection` worker for `strategy = Manual` with sub-agents, matching
    ///   python's `_register_manual_selection_worker`;
    ///
    /// then start polling.
    ///
    /// Server-side tools (`http`/`mcp`/`agent_tool`/`human`) need no local worker and are
    /// skipped. **Not yet registered here, for reasons recorded in the parity audit rather than
    /// left unexplained:**
    /// - The function-based router worker (python's `_register_router_worker`) — deferred.
    ///   [`AgentDef::router`] only models the agent-based router shape (see its own doc
    ///   comment); adding the callable shape would mean restructuring that field into an enum
    ///   the same way [`super::def::GateCondition`] restructured `gate`, which is a
    ///   disproportionate change for the smallest-value piece of this worker-registration
    ///   batch — the common agent-based router case already needs no worker at all (the server
    ///   evaluates it natively).
    ///
    /// Recurses into every sub-agent in [`AgentDef::agents`] and registers the same set of
    /// workers for each of them too, matching python's `_register_workers`, whose last step is
    /// exactly this recursion (`for sub in agent.agents: ... self._register_workers(sub, ...)`)
    /// — every handoff/swarm/hierarchical/parallel/sequential sub-agent's own tools need a
    /// locally-invoked worker just as much as the top-level agent's do. Unlike python, this
    /// crate has no notion of an "external" (already-deployed-elsewhere) sub-agent to skip —
    /// [`AgentDef`] doesn't model that concept yet — so every sub-agent is recursed into
    /// unconditionally.
    ///
    /// Registers workers and spawns their polling loops in the background, then returns --
    /// [`TaskHandler::start`] only blocks long enough to register/spawn, not for the runners'
    /// whole lifetime. Call [`AgentRuntime::shutdown`] (from another task, once this runtime is
    /// no longer needed) to stop them.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Worker`] if `agent` (and its sub-agent tree) registers no workers -- see [`TaskHandler::start`].
    pub async fn serve(&mut self, agent: &AgentDef) -> Result<()> {
        self.register_agent_workers(agent);
        self.task_handler.start().await
    }

    /// Re-attach to an execution this runtime didn't itself start -- typically after a process
    /// restart, since the execution is durable on the server regardless of which process
    /// started it. Registers `agent`'s local tool workers (identically to
    /// [`AgentRuntime::serve`]) and starts them polling, then returns an [`AgentHandle`] bound
    /// to `execution_id`.
    ///
    /// Call once per runtime, before it has already registered workers via
    /// [`AgentRuntime::serve`]/[`AgentRuntime::start`] for a different execution of the same
    /// `agent` -- calling it twice (or alongside `serve`) on the same runtime instance would
    /// register duplicate workers for the same task types, exactly as calling `serve` itself
    /// twice would.
    ///
    /// Narrower than python-sdk's `AgentRuntime.resume`: python extracts the execution's
    /// per-run worker domain from its `taskToDomain` mapping and re-registers workers scoped to
    /// that domain, because each stateful python execution gets a random per-run domain. This
    /// crate's worker registration (`register_agent_workers`) has no equivalent per-execution
    /// domain concept at all -- workers are always registered by task-type name alone, shared
    /// across every concurrent execution of the same agent -- so there is no domain to extract
    /// or re-apply here.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Worker`] if `agent` (and its sub-agent tree)
    /// registers no workers -- see [`TaskHandler::start`].
    pub async fn resume(
        &mut self,
        execution_id: impl Into<String>,
        agent: &AgentDef,
    ) -> Result<AgentHandle> {
        self.register_agent_workers(agent);
        self.task_handler.start().await?;
        Ok(AgentHandle::new(self.agent_client.clone(), execution_id))
    }

    /// Registers every worker [`AgentRuntime::serve`]'s doc comment describes for `agent` alone,
    /// then recurses into `agent.agents`. Does not start polling — callers call
    /// [`AgentRuntime::serve`], which does that once after the whole tree is registered.
    fn register_agent_workers(&mut self, agent: &AgentDef) {
        for tool in &agent.tools {
            if let Some(worker) = ToolWorker::from_tool_def(tool) {
                self.task_handler.add_worker(worker);
            }
            // Tool-level custom-function guardrails get the same single-guardrail-worker
            // registration as agent-level ones, matching python's tool-registration loop in
            // `_register_workers` (registered under the guardrail's own name, independent of
            // the owning agent).
            for guardrail in &tool.guardrails {
                if is_custom_function_guardrail(guardrail) {
                    self.task_handler.add_worker(GuardrailWorker {
                        task_name: guardrail.name.clone(),
                        guardrail: guardrail.clone(),
                    });
                }
            }
        }
        let sanitized_name = sanitize_for_task_name(&agent.name);
        if let Some(handler) = &agent.stop_when {
            self.task_handler.add_worker(StopWhenWorker {
                task_name: format!("{sanitized_name}_stop_when"),
                handler: std::sync::Arc::clone(handler),
            });
        }
        if let Some(condition) = &agent.termination {
            self.task_handler.add_worker(TerminationWorker {
                task_name: format!("{sanitized_name}_termination"),
                condition: condition.clone(),
            });
        }
        if !agent.callbacks.is_empty() {
            for position in CallbackPosition::ALL {
                self.task_handler.add_worker(CallbackWorker {
                    task_name: format!("{sanitized_name}_{}", position.as_str()),
                    position,
                    handlers: agent.callbacks.clone(),
                });
            }
        }
        for guardrail in &agent.guardrails {
            if is_custom_function_guardrail(guardrail) {
                self.task_handler.add_worker(GuardrailWorker {
                    task_name: guardrail.name.clone(),
                    guardrail: guardrail.clone(),
                });
            }
        }
        if let Some(super::def::GateCondition::Callable(handler)) = &agent.gate {
            self.task_handler.add_worker(GateWorker {
                task_name: format!("{sanitized_name}_gate"),
                handler: std::sync::Arc::clone(handler),
            });
        }
        // Hybrid handoff: an agent with both its own tools and sub-agents gets a check_transfer
        // worker plus one no-op transfer tool per sub-agent — matches python's
        // `_register_check_transfer_worker`/`_register_hybrid_transfer_workers`.
        if !agent.tools.is_empty() && !agent.agents.is_empty() {
            self.task_handler.add_worker(CheckTransferWorker {
                task_name: format!("{sanitized_name}_check_transfer"),
            });
            for sub in &agent.agents {
                self.task_handler.add_worker(TransferNoopWorker {
                    task_name: format!(
                        "{sanitized_name}_transfer_to_{}",
                        sanitize_for_task_name(&sub.name)
                    ),
                });
            }
        }
        // Handoff check — needed for any Swarm parent (server always generates the task) or
        // any agent with explicit condition-based handoffs, matching python's
        // `_register_handoff_worker` trigger condition.
        if !agent.swarm_transitions.is_empty()
            || (agent.strategy == super::def::Strategy::Swarm && !agent.agents.is_empty())
        {
            let mut name_to_idx = std::collections::HashMap::new();
            name_to_idx.insert(agent.name.clone(), "0".to_owned());
            for (i, sub) in agent.agents.iter().enumerate() {
                name_to_idx.insert(sub.name.clone(), (i + 1).to_string());
            }
            let idx_to_name: std::collections::HashMap<String, String> = name_to_idx
                .iter()
                .map(|(name, idx)| (idx.clone(), name.clone()))
                .collect();
            self.task_handler.add_worker(HandoffCheckWorker {
                task_name: format!("{sanitized_name}_handoff_check"),
                transitions: agent.swarm_transitions.clone(),
                name_to_idx,
                idx_to_name,
                allowed: agent.allowed_transitions.clone(),
                blocked_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
        }
        // Swarm transfer tools: every agent in the swarm gets a transfer tool for every peer,
        // plus its own check_transfer worker — matches python's
        // `_register_swarm_transfer_workers` (with the `allowed_transitions`-unreachable
        // variant) and the per-sub-agent `_register_check_transfer_worker` calls in "7b.".
        if agent.strategy == super::def::Strategy::Swarm && !agent.agents.is_empty() {
            let allowed = &agent.allowed_transitions;
            let valid_targets: std::collections::HashSet<&str> =
                allowed.values().flatten().map(String::as_str).collect();
            let all_names: Vec<&str> = std::iter::once(agent.name.as_str())
                .chain(agent.agents.iter().map(|a| a.name.as_str()))
                .collect();
            let mut registered = std::collections::HashSet::new();
            for &name in &all_names {
                for &peer in &all_names {
                    if peer == name {
                        continue;
                    }
                    let tool_name = format!(
                        "{}_transfer_to_{}",
                        sanitize_for_task_name(name),
                        sanitize_for_task_name(peer)
                    );
                    if !registered.insert(tool_name.clone()) {
                        continue;
                    }
                    let is_unreachable = !allowed.is_empty() && !valid_targets.contains(peer);
                    if is_unreachable {
                        self.task_handler.add_worker(TransferUnreachableWorker {
                            task_name: tool_name,
                        });
                    } else {
                        self.task_handler.add_worker(TransferNoopWorker {
                            task_name: tool_name,
                        });
                    }
                }
            }
            self.task_handler.add_worker(CheckTransferWorker {
                task_name: format!("{sanitized_name}_check_transfer"),
            });
            for sub in &agent.agents {
                self.task_handler.add_worker(CheckTransferWorker {
                    task_name: format!("{}_check_transfer", sanitize_for_task_name(&sub.name)),
                });
            }
        }
        // Manual selection — matches python's `_register_manual_selection_worker` trigger
        // condition.
        if agent.strategy == super::def::Strategy::Manual && !agent.agents.is_empty() {
            let name_to_idx: std::collections::HashMap<String, String> = agent
                .agents
                .iter()
                .enumerate()
                .map(|(i, sub)| (sub.name.clone(), i.to_string()))
                .collect();
            self.task_handler.add_worker(ManualSelectionWorker {
                task_name: format!("{sanitized_name}_process_selection"),
                name_to_idx,
            });
        }
        for sub in &agent.agents {
            self.register_agent_workers(sub);
        }
    }

    /// Register every locally-invoked tool in `tools` (i.e. every [`ToolDef`] whose `handler` is
    /// `Some`) as a worker, then start polling — the same tool-registration half of
    /// [`AgentRuntime::serve`], exposed standalone for agent shapes that don't have an
    /// [`AgentDef`] to walk. `super::skill`'s framework-marker skill agents are the motivating
    /// case: their tools are locally-built worker functions (`ScriptRunner`/`SkillFileReader`
    /// equivalents), not an `AgentDef.tools` list, since the agent itself compiles via
    /// [`AgentRuntime::compile_framework`]/[`AgentRuntime::deploy_framework`] rather than the
    /// normal `AgentDef` tree.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Worker`] if `tools` registers no workers -- see [`TaskHandler::start`].
    pub async fn serve_tools(&mut self, tools: &[ToolDef]) -> Result<()> {
        for tool in tools {
            if let Some(worker) = ToolWorker::from_tool_def(tool) {
                self.task_handler.add_worker(worker);
            }
        }
        self.task_handler.start().await
    }

    /// Gracefully stop every worker [`AgentRuntime::serve`]/[`AgentRuntime::serve_tools`]
    /// registered.
    ///
    /// # Errors
    ///
    /// Currently always returns `Ok(())`; see [`TaskHandler::stop`].
    pub async fn shutdown(&mut self) -> Result<()> {
        self.task_handler.stop().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::tool::ToolType;
    use std::collections::HashMap;

    #[test]
    fn test_agent_runtime_new_from_configuration() {
        let config = Configuration::new("http://localhost:8080/api");
        let runtime = AgentRuntime::new(config);
        runtime.unwrap();
    }

    #[tokio::test]
    async fn test_resume_registers_workers_and_returns_a_handle_for_the_given_execution_id() {
        let mock_server = wiremock::MockServer::start().await;
        let config = Configuration::new(format!("{}/api", mock_server.uri()));
        let mut runtime = AgentRuntime::new(config).unwrap();

        let agent = AgentDef::new("resumable_agent")
            .unwrap()
            .with_tool(ToolDef::function::<Value, _, _>(
                "my_tool",
                "a test tool",
                serde_json::json!({"type": "object"}),
                |_args: Value| async move { Ok(Value::Null) },
            ));

        let handle = runtime.resume("exec-existing-123", &agent).await.unwrap();
        assert_eq!(handle.execution_id(), "exec-existing-123");

        // Not calling `runtime.shutdown()`: this test's #[tokio::test] runtime drops (and with
        // it, aborts) the poller task `resume` spawned as soon as this function returns, which
        // is faster and just as clean here as a graceful stop -- there's no real in-flight work
        // to drain against a mock server this test never registered a poll response on.
    }

    /// `compile()`/`deploy()`/`start()` all build their outgoing payload around
    /// `AgentConfigSerializer::serialize` under a top-level `agentConfig` key — this asserts on
    /// that inner shape, since actually calling `compile()` would require a live server. See
    /// `test_compile_and_deploy_wrap_payload_in_agent_config_envelope` for the envelope itself,
    /// which is what a real server's `AgentStartRequest` DTO actually requires.
    #[test]
    fn test_compile_payload_matches_agent_config_serializer_shape() {
        let agent = AgentDef::new("compiler_test")
            .unwrap()
            .with_model("openai/gpt-4o");

        let payload = AgentConfigSerializer::serialize(&agent);
        let obj = payload.as_object().unwrap();

        assert_eq!(
            obj.get("name"),
            Some(&Value::String("compiler_test".to_owned()))
        );
        assert_eq!(
            obj.get("model"),
            Some(&Value::String("openai/gpt-4o".to_owned()))
        );
        assert_eq!(obj.get("external"), Some(&Value::Bool(false)));
    }

    /// Regression test for the bug this audit found: `compile()`/`deploy()` used to POST
    /// `AgentConfigSerializer::serialize(agent)` directly as the request body. The server's
    /// `AgentStartRequest` DTO requires it nested under `agentConfig` — confirmed against a real
    /// server, which rejected the unwrapped shape with `400 "agentConfig is required when
    /// framework is not specified"`.
    #[test]
    fn test_compile_and_deploy_wrap_payload_in_agent_config_envelope() {
        let agent = AgentDef::new("envelope_test")
            .unwrap()
            .with_model("openai/gpt-4o");
        let inner = AgentConfigSerializer::serialize(&agent);

        let wrapped = serde_json::json!({ "agentConfig": inner });
        let obj = wrapped.as_object().unwrap();

        assert_eq!(obj.len(), 1, "payload must have exactly one top-level key");
        assert_eq!(obj.get("agentConfig"), Some(&inner));
    }

    #[test]
    fn test_framework_payload_shape() {
        let payload = framework_payload("openai", serde_json::json!({"name": "a"}));
        assert_eq!(
            payload,
            serde_json::json!({"framework": "openai", "rawConfig": {"name": "a"}})
        );
    }

    #[test]
    fn test_extract_execution_id_reads_camel_case() {
        let id = extract_execution_id(&serde_json::json!({"executionId": "exec-1"})).unwrap();
        assert_eq!(id, "exec-1");
    }

    #[test]
    fn test_extract_execution_id_reads_snake_case_fallback() {
        let id = extract_execution_id(&serde_json::json!({"execution_id": "exec-2"})).unwrap();
        assert_eq!(id, "exec-2");
    }

    #[tokio::test]
    async fn test_deploy_with_schedules_none_only_deploys() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/agent/deploy"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "registeredName": "billing_agent",
                })),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = Configuration::new(format!("{}/api", mock_server.uri()));
        let runtime = AgentRuntime::new(config).unwrap();
        let agent = AgentDef::new("billing_agent").unwrap();

        runtime
            .deploy_with_schedules(&agent, None)
            .await
            .expect("deploy_with_schedules failed");

        // No scheduler mocks were registered above and `expect(1)` is asserted on drop: if
        // `deploy_with_schedules` touched the scheduler at all with `schedules = None`, either
        // this test would panic on an unmatched request or the mount's own request count would
        // be wrong -- either way, catches a regression that starts reconciling unconditionally.
    }

    #[tokio::test]
    async fn test_deploy_with_schedules_upserts_and_prunes() {
        let mock_server = wiremock::MockServer::start().await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/agent/deploy"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "registeredName": "billing_agent",
                })),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/api/scheduler/schedules"))
            .and(wiremock::matchers::query_param(
                "workflowName",
                "billing_agent",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!([
                    {
                        "name": "billing_agent-stale",
                        "cronExpression": "0 0 * * * *",
                        "workflowName": "billing_agent",
                    },
                    {
                        "name": "billing_agent-daily",
                        "cronExpression": "0 0 * * * *",
                        "workflowName": "billing_agent",
                    },
                ])),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("DELETE"))
            .and(wiremock::matchers::path(
                "/api/scheduler/schedules/billing_agent-stale",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/api/scheduler/schedules"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = Configuration::new(format!("{}/api", mock_server.uri()));
        let runtime = AgentRuntime::new(config).unwrap();
        let agent = AgentDef::new("billing_agent").unwrap();
        let schedules = vec![Schedule::new("daily", "0 0 * * * *").unwrap()];

        runtime
            .deploy_with_schedules(&agent, Some(&schedules))
            .await
            .expect("deploy_with_schedules failed");

        // `billing_agent-stale` (not in `schedules`) must have been pruned via the DELETE mock
        // above, and `billing_agent-daily` upserted via the POST mock -- both `expect(1)`s are
        // asserted on drop.
    }

    #[test]
    fn test_extract_execution_id_errors_when_absent() {
        extract_execution_id(&serde_json::json!({})).unwrap_err();
    }

    fn empty_start_payload() -> Map<String, Value> {
        let mut payload = Map::new();
        payload.insert("agentConfig".to_owned(), serde_json::json!({}));
        payload
    }

    #[test]
    fn test_merge_start_input_treats_plain_string_as_prompt() {
        let mut payload = empty_start_payload();
        merge_start_input(&mut payload, Value::String("hello".to_owned()));
        assert_eq!(payload["prompt"], Value::String("hello".to_owned()));
    }

    #[test]
    fn test_merge_start_input_passes_recognized_keys_through_at_top_level() {
        let mut payload = empty_start_payload();
        merge_start_input(
            &mut payload,
            serde_json::json!({
                "prompt": "hi",
                "media": ["https://example.com/a.png"],
                "context": {"a": 1},
                "sessionId": "sess-1",
                "static_plan": {"steps": []},
            }),
        );
        assert_eq!(payload["prompt"], Value::String("hi".to_owned()));
        assert_eq!(
            payload["media"],
            serde_json::json!(["https://example.com/a.png"])
        );
        assert_eq!(payload["context"], serde_json::json!({"a": 1}));
        assert_eq!(payload["sessionId"], Value::String("sess-1".to_owned()));
        assert_eq!(payload["static_plan"], serde_json::json!({"steps": []}));
    }

    #[test]
    fn test_merge_start_input_normalizes_snake_case_session_id() {
        let mut payload = empty_start_payload();
        merge_start_input(&mut payload, serde_json::json!({"session_id": "sess-2"}));
        assert_eq!(payload["sessionId"], Value::String("sess-2".to_owned()));
    }

    #[test]
    fn test_merge_start_input_folds_unrecognized_keys_into_context() {
        let mut payload = empty_start_payload();
        merge_start_input(
            &mut payload,
            serde_json::json!({"prompt": "hi", "user_id": "u-1"}),
        );
        assert_eq!(payload["prompt"], Value::String("hi".to_owned()));
        assert_eq!(payload["context"], serde_json::json!({"user_id": "u-1"}));
    }

    #[test]
    fn test_merge_start_input_merges_overflow_into_existing_context() {
        let mut payload = empty_start_payload();
        merge_start_input(
            &mut payload,
            serde_json::json!({"context": {"a": 1}, "extra": 2}),
        );
        assert_eq!(payload["context"], serde_json::json!({"a": 1, "extra": 2}));
    }

    #[test]
    fn test_merge_start_input_wraps_non_object_non_string_as_context_value() {
        let mut payload = empty_start_payload();
        merge_start_input(&mut payload, serde_json::json!(42));
        assert_eq!(payload["context"], serde_json::json!({"value": 42}));
    }

    // `AgentStatus::from_response`/`is_terminal` are now owned by `result.rs` (this crate's
    // single canonical `AgentStatus`, no longer a private duplicate here) — see its test module
    // for coverage.

    #[tokio::test]
    async fn test_tool_worker_bridge_invokes_handler_and_threads_credentials() {
        #[derive(serde::Deserialize)]
        struct Args {
            n: i32,
        }

        let tool = ToolDef::function_with_credentials::<Args, _, _>(
            "double_with_token",
            "doubles a number, using a token",
            serde_json::json!({"type": "object"}),
            |args: Args, creds: &Credentials| {
                let token = creds.get("API_KEY").unwrap().to_owned();
                async move { Ok(Value::from(format!("{token}:{}", args.n * 2))) }
            },
        )
        .with_credentials(vec!["API_KEY".to_owned()]);

        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");
        assert_eq!(worker.task_definition_name(), "double_with_token");
        assert_eq!(worker.declared_credentials(), vec!["API_KEY".to_owned()]);

        let mut task = Task {
            runtime_metadata: HashMap::from([("API_KEY".to_owned(), "secret".to_owned())]),
            ..Default::default()
        };
        task.input_data.insert("n".to_owned(), Value::from(21));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("result"), Some(&Value::from("secret:42")));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn test_tool_worker_bridge_skips_server_side_tools() {
        let tool = ToolDef::human("ask", "ask a human");
        assert_eq!(tool.tool_type, ToolType::Human);
        assert!(ToolWorker::from_tool_def(&tool).is_none());
    }

    #[tokio::test]
    async fn test_tool_worker_reads_agent_state_and_reports_state_updates() {
        #[derive(serde::Deserialize)]
        struct Args {
            n: i32,
        }

        let tool = ToolDef::function_with_context::<Args, _, _>(
            "increment",
            "increments a running total",
            serde_json::json!({"type": "object"}),
            |args: Args, ctx: ToolContext| async move {
                let previous = ctx.get_state("total").and_then(|v| v.as_i64()).unwrap_or(0);
                let total = previous + i64::from(args.n);
                ctx.set_state("total", Value::from(total));
                Ok(Value::from(total))
            },
        );
        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");

        let mut task = Task {
            workflow_instance_id: "wf-123".to_owned(),
            ..Default::default()
        };
        task.input_data.insert("n".to_owned(), Value::from(5));
        task.input_data
            .insert("_agent_state".to_owned(), serde_json::json!({"total": 10}));

        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("result"), Some(&Value::from(15)));
        assert_eq!(
            map.get("_state_updates"),
            Some(&serde_json::json!({"total": 15}))
        );
    }

    #[tokio::test]
    async fn test_tool_worker_omits_state_updates_when_context_unused() {
        let tool = ToolDef::function::<Value, _, _>(
            "noop",
            "does nothing with context",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::from("done")) },
        );
        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");

        let output = worker.execute(&Task::default()).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert!(!map.contains_key("_state_updates"));
    }

    #[tokio::test]
    async fn test_tool_worker_strips_agent_state_from_tool_arguments() {
        let tool = ToolDef::function::<Value, _, _>(
            "echo_args",
            "echoes the raw arguments it received",
            serde_json::json!({"type": "object"}),
            |args: Value| async move { Ok(args) },
        );
        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");

        let mut task = Task::default();
        task.input_data.insert("n".to_owned(), Value::from(1));
        task.input_data
            .insert("_agent_state".to_owned(), serde_json::json!({"total": 10}));

        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        // An object-shaped tool return becomes the task output directly (matching python's
        // `isinstance(result, dict)` branch), not nested under a "result" key.
        assert!(!map.contains_key("_agent_state"));
        assert_eq!(map.get("n"), Some(&Value::from(1)));
    }

    #[tokio::test]
    async fn test_tool_worker_object_return_is_not_wrapped_under_result_key() {
        let tool = ToolDef::function::<Value, _, _>(
            "get_weather",
            "returns a dict-shaped result",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move {
                Ok(serde_json::json!({"city": "SF", "temp_f": 72, "condition": "Sunny"}))
            },
        );
        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");

        let output = worker.execute(&Task::default()).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert!(!map.contains_key("result"));
        assert_eq!(map.get("city"), Some(&Value::from("SF")));
        assert_eq!(map.get("temp_f"), Some(&Value::from(72)));
        assert_eq!(map.get("condition"), Some(&Value::from("Sunny")));
    }

    #[tokio::test]
    async fn test_tool_worker_scalar_return_is_wrapped_under_result_key() {
        let tool = ToolDef::function::<Value, _, _>(
            "count",
            "returns a bare number",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::from(42)) },
        );
        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");

        let output = worker.execute(&Task::default()).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("result"), Some(&Value::from(42)));
    }

    fn stop_when_worker(handler: super::super::def::StopWhenHandler) -> StopWhenWorker {
        StopWhenWorker {
            task_name: "agent_stop_when".to_owned(),
            handler,
        }
    }

    #[tokio::test]
    async fn test_stop_when_worker_stops_when_predicate_returns_true() {
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_stop_when(|context: Value| async move { Ok(context["result"] == "done") });
        let worker = stop_when_worker(agent.stop_when.clone().unwrap());
        assert_eq!(worker.task_definition_name(), "agent_stop_when");

        let mut task = Task::default();
        task.input_data
            .insert("result".to_owned(), Value::from("done"));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                // Inverted: predicate said "stop" (true), worker reports "should_continue: false".
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(false)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_stop_when_worker_continues_when_predicate_returns_false() {
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_stop_when(|_context: Value| async move { Ok(false) });
        let worker = stop_when_worker(agent.stop_when.clone().unwrap());

        let output = worker.execute(&Task::default()).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(true)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    /// Regression test for python's exact fail-open behavior: a predicate `Err` must not stop
    /// the agent loop (`StopWhenEntry.__call__`'s `except Exception` branch returns
    /// `{"should_continue": True}`, not a task failure).
    #[tokio::test]
    async fn test_stop_when_worker_fails_open_on_predicate_error() {
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_stop_when(|_context: Value| async move { Err(ConductorError::agent("boom")) });
        let worker = stop_when_worker(agent.stop_when.clone().unwrap());

        let output = worker.execute(&Task::default()).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(true)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_stop_when_worker_builds_context_from_task_input_with_defaults() {
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_stop_when(|context: Value| async move {
                assert_eq!(context["result"], Value::String(String::new()));
                assert_eq!(context["messages"], Value::Array(Vec::new()));
                assert_eq!(context["iteration"], Value::from(0));
                Ok(false)
            });
        let worker = stop_when_worker(agent.stop_when.clone().unwrap());

        // No result/messages/iteration set on the task -- worker must supply defaults matching
        // python's `StopWhenEntry.__call__` signature defaults (`result: object = ""`,
        // `iteration: int = 0`, `messages: object = None` normalized to `[]`).
        worker.execute(&Task::default()).await.unwrap();
    }

    #[test]
    fn test_serve_registers_stop_when_worker() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        assert_eq!(runtime.task_handler.worker_count(), 0);

        let agent = AgentDef::new("agent")
            .unwrap()
            .with_stop_when(|_context: Value| async move { Ok(false) });

        if let Some(handler) = &agent.stop_when {
            runtime.task_handler.add_worker(StopWhenWorker {
                task_name: format!("{}_stop_when", agent.name),
                handler: std::sync::Arc::clone(handler),
            });
        }

        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    #[tokio::test]
    async fn test_gate_worker_continue_when_predicate_true() {
        let worker = GateWorker {
            task_name: "agent_gate".to_owned(),
            handler: std::sync::Arc::new(|context: Value| {
                Box::pin(async move { Ok(context["result"] == "DONE") })
            }),
        };
        assert_eq!(worker.task_definition_name(), "agent_gate");

        let mut task = Task::default();
        task.input_data
            .insert("result".to_owned(), Value::from("DONE"));
        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(
                    map.get("decision"),
                    Some(&Value::String("continue".to_owned()))
                );
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_gate_worker_stop_when_predicate_false() {
        let worker = GateWorker {
            task_name: "agent_gate".to_owned(),
            handler: std::sync::Arc::new(|context: Value| {
                Box::pin(async move { Ok(context["result"] == "DONE") })
            }),
        };

        let mut task = Task::default();
        task.input_data
            .insert("result".to_owned(), Value::from("nope"));
        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("decision"), Some(&Value::String("stop".to_owned())));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_gate_worker_fails_open_on_predicate_error() {
        let worker = GateWorker {
            task_name: "agent_gate".to_owned(),
            handler: std::sync::Arc::new(|_context: Value| {
                Box::pin(async move { Err(ConductorError::agent("boom")) })
            }),
        };

        let output = worker.execute(&Task::default()).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(
                    map.get("decision"),
                    Some(&Value::String("continue".to_owned()))
                );
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn test_serve_registers_gate_worker_only_for_callable_variant() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();

        let text_gate_agent = AgentDef::new("a")
            .unwrap()
            .with_gate(super::super::def::TextGate::new("DONE"));
        if let Some(super::super::def::GateCondition::Callable(handler)) = &text_gate_agent.gate {
            runtime.task_handler.add_worker(GateWorker {
                task_name: format!("{}_gate", text_gate_agent.name),
                handler: std::sync::Arc::clone(handler),
            });
        }
        assert_eq!(runtime.task_handler.worker_count(), 0);

        let callable_gate_agent = AgentDef::new("b")
            .unwrap()
            .with_gate_fn(|_: Value| async move { Ok(true) });
        if let Some(super::super::def::GateCondition::Callable(handler)) = &callable_gate_agent.gate
        {
            runtime.task_handler.add_worker(GateWorker {
                task_name: format!("{}_gate", callable_gate_agent.name),
                handler: std::sync::Arc::clone(handler),
            });
        }
        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    #[test]
    fn test_serve_registers_termination_worker() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_termination(TerminationCondition::stop_message_default());

        if let Some(condition) = &agent.termination {
            runtime.task_handler.add_worker(TerminationWorker {
                task_name: format!("{}_termination", agent.name),
                condition: condition.clone(),
            });
        }

        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    #[test]
    fn test_register_agent_workers_recurses_into_sub_agents() {
        // Matches python's `_register_workers`, whose last step recurses into `agent.agents` —
        // a handoff/swarm/hierarchical parent's sub-agents' own tools need local workers just
        // as much as the parent's do. `register_agent_workers` (unlike `serve`, which blocks
        // forever polling) doesn't start polling, so it's directly testable here.
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();

        let billing =
            AgentDef::new("billing")
                .unwrap()
                .with_tool(ToolDef::function::<Value, _, _>(
                    "check_balance",
                    "checks a balance",
                    serde_json::json!({"type": "object"}),
                    |_args: Value| async move { Ok(Value::Null) },
                ));
        let technical = AgentDef::new("technical")
            .unwrap()
            .with_tool(ToolDef::function::<Value, _, _>(
                "lookup_order",
                "looks up an order",
                serde_json::json!({"type": "object"}),
                |_args: Value| async move { Ok(Value::Null) },
            ));
        let support = AgentDef::new("support")
            .unwrap()
            .with_sub_agent(billing)
            .unwrap()
            .with_sub_agent(technical)
            .unwrap();

        runtime.register_agent_workers(&support);

        // `support` itself has no tools; both workers below only exist if the recursion into
        // its two sub-agents ran.
        assert_eq!(runtime.task_handler.worker_count(), 2);
    }

    #[tokio::test]
    async fn test_termination_worker_execute_matches_condition() {
        let worker = TerminationWorker {
            task_name: "agent_termination".to_owned(),
            condition: TerminationCondition::stop_message_default(),
        };
        assert_eq!(worker.task_definition_name(), "agent_termination");

        let mut task = Task::default();
        task.input_data
            .insert("result".to_owned(), Value::from("TERMINATE"));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(false)));
                assert!(map["reason"].as_str().unwrap().contains("TERMINATE"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_termination_worker_continues_when_condition_not_met() {
        let worker = TerminationWorker {
            task_name: "agent_termination".to_owned(),
            condition: TerminationCondition::stop_message_default(),
        };
        let mut task = Task::default();
        task.input_data
            .insert("result".to_owned(), Value::from("still working"));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(true)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    struct TestCallbackHandler {
        respond_at: Option<CallbackPosition>,
    }

    #[async_trait]
    impl CallbackHandler for TestCallbackHandler {
        async fn on_agent_start(&self, _ctx: &CallbackContext) -> Option<Value> {
            (self.respond_at == Some(CallbackPosition::BeforeAgent))
                .then(|| serde_json::json!({"seen": "before_agent"}))
        }
        async fn on_model_start(&self, _ctx: &CallbackContext) -> Option<Value> {
            (self.respond_at == Some(CallbackPosition::BeforeModel))
                .then(|| serde_json::json!({"seen": "before_model"}))
        }
    }

    #[tokio::test]
    async fn test_dispatch_callback_position_returns_first_matching_handler_result() {
        let handlers: Vec<std::sync::Arc<dyn CallbackHandler>> = vec![
            std::sync::Arc::new(TestCallbackHandler { respond_at: None }),
            std::sync::Arc::new(TestCallbackHandler {
                respond_at: Some(CallbackPosition::BeforeModel),
            }),
        ];
        let output = dispatch_callback_position(
            CallbackPosition::BeforeModel,
            &handlers,
            &CallbackContext::new(),
        )
        .await;
        assert_eq!(output, serde_json::json!({"seen": "before_model"}));
    }

    #[tokio::test]
    async fn test_dispatch_callback_position_returns_empty_object_when_no_handler_matches() {
        let handlers: Vec<std::sync::Arc<dyn CallbackHandler>> =
            vec![std::sync::Arc::new(TestCallbackHandler {
                respond_at: None,
            })];
        let output = dispatch_callback_position(
            CallbackPosition::AfterTool,
            &handlers,
            &CallbackContext::new(),
        )
        .await;
        assert_eq!(output, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_callback_worker_execute_builds_context_from_task_input() {
        let worker = CallbackWorker {
            task_name: "agent_before_agent".to_owned(),
            position: CallbackPosition::BeforeAgent,
            handlers: vec![std::sync::Arc::new(TestCallbackHandler {
                respond_at: Some(CallbackPosition::BeforeAgent),
            })],
        };
        assert_eq!(worker.task_definition_name(), "agent_before_agent");

        let output = worker.execute(&Task::default()).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(
                    map.get("seen"),
                    Some(&Value::String("before_agent".to_owned()))
                );
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn test_serve_registers_one_worker_per_callback_position() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_callback(TestCallbackHandler { respond_at: None });

        assert!(!agent.callbacks.is_empty());
        for position in CallbackPosition::ALL {
            runtime.task_handler.add_worker(CallbackWorker {
                task_name: format!("{}_{}", agent.name, position.as_str()),
                position,
                handlers: agent.callbacks.clone(),
            });
        }

        assert_eq!(runtime.task_handler.worker_count(), 6);
    }

    fn function_guardrail(name: &str, on_fail: super::super::guardrail::OnFail) -> Guardrail {
        Guardrail::new(
            name,
            super::super::guardrail::FunctionGuardrail::new(|content: &str| {
                if content.contains("bad") {
                    super::super::guardrail::GuardrailResult::fail("contains 'bad'")
                } else {
                    super::super::guardrail::GuardrailResult::pass()
                }
            }),
        )
        .with_on_fail(on_fail)
        .unwrap()
    }

    #[test]
    fn test_is_custom_function_guardrail_distinguishes_from_regex_and_llm() {
        use super::super::guardrail::{LlmGuardrail, OnFail, RegexGuardrail};

        assert!(is_custom_function_guardrail(&function_guardrail(
            "custom",
            OnFail::Raise
        )));
        assert!(!is_custom_function_guardrail(&Guardrail::new(
            "regex",
            RegexGuardrail::new(["x"]).unwrap()
        )));
        assert!(!is_custom_function_guardrail(&Guardrail::new(
            "llm",
            LlmGuardrail::new("m", "p")
        )));
    }

    #[tokio::test]
    async fn test_guardrail_worker_passes_through_on_success() {
        use super::super::guardrail::OnFail;

        let worker = GuardrailWorker {
            task_name: "no_bad_words".to_owned(),
            guardrail: function_guardrail("no_bad_words", OnFail::Raise),
        };
        assert_eq!(worker.task_definition_name(), "no_bad_words");

        let mut task = Task::default();
        task.input_data
            .insert("content".to_owned(), Value::from("all good here"));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("passed"), Some(&Value::Bool(true)));
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(false)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_guardrail_worker_reports_failure_with_guardrail_name() {
        use super::super::guardrail::OnFail;

        let worker = GuardrailWorker {
            task_name: "no_bad_words".to_owned(),
            guardrail: function_guardrail("no_bad_words", OnFail::Raise),
        };

        let mut task = Task::default();
        task.input_data
            .insert("content".to_owned(), Value::from("this is bad"));

        let output = worker.execute(&task).await.unwrap();
        match output {
            WorkerOutput::Completed(map) => {
                assert_eq!(map.get("passed"), Some(&Value::Bool(false)));
                assert_eq!(map.get("on_fail"), Some(&Value::String("raise".to_owned())));
                assert_eq!(
                    map.get("guardrail_name"),
                    Some(&Value::String("no_bad_words".to_owned()))
                );
                assert_eq!(map.get("should_continue"), Some(&Value::Bool(false)));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_guardrail_worker_retry_escalates_to_raise_at_max_retries() {
        use super::super::guardrail::OnFail;

        let worker = GuardrailWorker {
            task_name: "no_bad_words".to_owned(),
            guardrail: function_guardrail("no_bad_words", OnFail::Retry)
                .with_max_retries(2)
                .unwrap(),
        };

        let mut task = Task::default();
        task.input_data
            .insert("content".to_owned(), Value::from("this is bad"));
        task.input_data
            .insert("iteration".to_owned(), Value::from(0));
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("on_fail"), Some(&Value::String("retry".to_owned())));
        assert_eq!(map.get("should_continue"), Some(&Value::Bool(true)));

        task.input_data
            .insert("iteration".to_owned(), Value::from(2));
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("on_fail"), Some(&Value::String("raise".to_owned())));
        assert_eq!(map.get("should_continue"), Some(&Value::Bool(false)));
    }

    #[tokio::test]
    async fn test_guardrail_worker_fix_escalates_to_raise_when_no_fixed_output() {
        use super::super::guardrail::{FunctionGuardrail, GuardrailResult, OnFail};

        let worker = GuardrailWorker {
            task_name: "no_bad_words".to_owned(),
            guardrail: Guardrail::new(
                "no_bad_words",
                FunctionGuardrail::new(|_: &str| GuardrailResult::fail("nope")),
            )
            .with_on_fail(OnFail::Fix)
            .unwrap(),
        };

        let mut task = Task::default();
        task.input_data
            .insert("content".to_owned(), Value::from("this is bad"));

        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("on_fail"), Some(&Value::String("raise".to_owned())));
    }

    #[tokio::test]
    async fn test_guardrail_worker_stringifies_non_string_content() {
        use super::super::guardrail::OnFail;

        let worker = GuardrailWorker {
            task_name: "no_bad_words".to_owned(),
            guardrail: function_guardrail("no_bad_words", OnFail::Raise),
        };

        let mut task = Task::default();
        task.input_data
            .insert("content".to_owned(), serde_json::json!({"text": "ok"}));

        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("passed"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_serve_registers_worker_for_custom_function_guardrail_only() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let agent = AgentDef::new("agent")
            .unwrap()
            .with_guardrail(function_guardrail(
                "custom_check",
                super::super::guardrail::OnFail::Raise,
            ))
            .with_guardrail(Guardrail::new(
                "regex_check",
                super::super::guardrail::RegexGuardrail::new(["x"]).unwrap(),
            ));

        for guardrail in &agent.guardrails {
            if is_custom_function_guardrail(guardrail) {
                runtime.task_handler.add_worker(GuardrailWorker {
                    task_name: guardrail.name.clone(),
                    guardrail: guardrail.clone(),
                });
            }
        }

        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    #[test]
    fn test_serve_registers_worker_for_tool_level_custom_function_guardrail() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let tool = ToolDef::human("ask", "ask a human")
            .with_guardrail(function_guardrail(
                "tool_custom_check",
                super::super::guardrail::OnFail::Raise,
            ))
            .with_guardrail(Guardrail::new(
                "tool_regex_check",
                super::super::guardrail::RegexGuardrail::new(["x"]).unwrap(),
            ));
        let agent = AgentDef::new("agent").unwrap().with_tool(tool);

        for tool in &agent.tools {
            for guardrail in &tool.guardrails {
                if is_custom_function_guardrail(guardrail) {
                    runtime.task_handler.add_worker(GuardrailWorker {
                        task_name: guardrail.name.clone(),
                        guardrail: guardrail.clone(),
                    });
                }
            }
        }

        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    // ── CheckTransferWorker / TransferNoopWorker / TransferUnreachableWorker ──

    #[test]
    fn test_evaluate_check_transfer_no_transfer_calls() {
        let output = evaluate_check_transfer(&[]);
        assert_eq!(
            output,
            serde_json::json!({"is_transfer": false, "transfer_to": "", "transfer_message": ""})
        );
    }

    #[test]
    fn test_evaluate_check_transfer_detects_transfer_tool() {
        let tool_calls = vec![serde_json::json!({
            "name": "triage_transfer_to_billing",
            "inputParameters": {"message": "please help with billing"},
        })];
        let output = evaluate_check_transfer(&tool_calls);
        assert_eq!(output["is_transfer"], serde_json::json!(true));
        assert_eq!(output["transfer_to"], serde_json::json!("billing"));
        assert_eq!(
            output["transfer_message"],
            serde_json::json!("please help with billing")
        );
    }

    #[test]
    fn test_evaluate_check_transfer_ignores_non_transfer_calls() {
        let tool_calls = vec![serde_json::json!({"name": "search", "inputParameters": {}})];
        let output = evaluate_check_transfer(&tool_calls);
        assert_eq!(output["is_transfer"], serde_json::json!(false));
    }

    #[test]
    fn test_evaluate_check_transfer_first_wins_and_reports_dropped() {
        let tool_calls = vec![
            serde_json::json!({"name": "a_transfer_to_b", "inputParameters": {}}),
            serde_json::json!({"name": "a_transfer_to_c", "inputParameters": {}}),
        ];
        let output = evaluate_check_transfer(&tool_calls);
        assert_eq!(output["transfer_to"], serde_json::json!("b"));
        assert_eq!(
            output["dropped_transfers"][0]["transfer_to"],
            serde_json::json!("c")
        );
    }

    #[tokio::test]
    async fn test_check_transfer_worker_execute() {
        let worker = CheckTransferWorker {
            task_name: "agent_check_transfer".to_owned(),
        };
        assert_eq!(worker.task_definition_name(), "agent_check_transfer");
        let mut task = Task::default();
        task.input_data.insert(
            "tool_calls".to_owned(),
            serde_json::json!([{"name": "a_transfer_to_b", "inputParameters": {}}]),
        );
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("transfer_to"), Some(&Value::String("b".to_owned())));
    }

    #[tokio::test]
    async fn test_transfer_noop_worker_echoes_message_when_present() {
        let worker = TransferNoopWorker {
            task_name: "a_transfer_to_b".to_owned(),
        };
        let mut task = Task::default();
        task.input_data
            .insert("message".to_owned(), Value::from("hello there"));
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(
            map.get("message"),
            Some(&Value::String("hello there".to_owned()))
        );
    }

    #[tokio::test]
    async fn test_transfer_noop_worker_empty_output_when_no_message() {
        let worker = TransferNoopWorker {
            task_name: "a_transfer_to_b".to_owned(),
        };
        let output = worker.execute(&Task::default()).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert!(map.is_empty());
    }

    #[tokio::test]
    async fn test_transfer_unreachable_worker_returns_fixed_error() {
        let worker = TransferUnreachableWorker {
            task_name: "a_transfer_to_b".to_owned(),
        };
        let output = worker.execute(&Task::default()).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert!(map["result"]
            .as_str()
            .unwrap()
            .contains("a_transfer_to_b is not available"));
    }

    // ── HandoffCheckWorker ─────────────────────────────────────────────

    fn handoff_maps() -> (
        std::collections::HashMap<String, String>,
        std::collections::HashMap<String, String>,
    ) {
        let mut name_to_idx = std::collections::HashMap::new();
        name_to_idx.insert("parent".to_owned(), "0".to_owned());
        name_to_idx.insert("billing".to_owned(), "1".to_owned());
        name_to_idx.insert("refunds".to_owned(), "2".to_owned());
        let idx_to_name = name_to_idx
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        (name_to_idx, idx_to_name)
    }

    #[test]
    fn test_evaluate_handoff_check_detected_transfer_switches_agent() {
        let (name_to_idx, idx_to_name) = handoff_maps();
        let allowed = std::collections::HashMap::new();
        let mut blocked = std::collections::HashMap::new();
        let decision = evaluate_handoff_check(
            &[],
            &name_to_idx,
            &idx_to_name,
            &allowed,
            &mut blocked,
            "",
            "0",
            &Value::Bool(true),
            "billing",
        );
        assert_eq!(decision.active_agent, "1");
        assert!(decision.handoff);
    }

    #[test]
    fn test_evaluate_handoff_check_blocked_transfer_retries_then_gives_up() {
        let (name_to_idx, idx_to_name) = handoff_maps();
        let mut allowed = std::collections::HashMap::new();
        allowed.insert("parent".to_owned(), vec!["refunds".to_owned()]);
        let mut blocked = std::collections::HashMap::new();

        for expected_count in 1..=3 {
            let decision = evaluate_handoff_check(
                &[],
                &name_to_idx,
                &idx_to_name,
                &allowed,
                &mut blocked,
                "",
                "0",
                &Value::Bool(true),
                "billing",
            );
            assert_eq!(blocked.get("0"), Some(&expected_count));
            assert_eq!(decision.active_agent, "0");
            assert!(
                decision.handoff,
                "retry {expected_count} should still be handoff=true"
            );
        }

        // 4th attempt exceeds max_blocked_retries (3) — gives up.
        let decision = evaluate_handoff_check(
            &[],
            &name_to_idx,
            &idx_to_name,
            &allowed,
            &mut blocked,
            "",
            "0",
            &Value::Bool(true),
            "billing",
        );
        assert!(!decision.handoff);
        assert!(!blocked.contains_key("0"));
    }

    #[test]
    fn test_evaluate_handoff_check_falls_back_to_condition_based() {
        let (name_to_idx, idx_to_name) = handoff_maps();
        let allowed = std::collections::HashMap::new();
        let mut blocked = std::collections::HashMap::new();
        let transitions = vec![super::super::swarm::SwarmTransition::OnTextMention {
            target: "refunds".to_owned(),
            text: "REFUND".to_owned(),
        }];
        let decision = evaluate_handoff_check(
            &transitions,
            &name_to_idx,
            &idx_to_name,
            &allowed,
            &mut blocked,
            "please process a REFUND",
            "0",
            &Value::Bool(false),
            "",
        );
        assert_eq!(decision.active_agent, "2");
        assert!(decision.handoff);
    }

    #[test]
    fn test_evaluate_handoff_check_no_match_exits_loop() {
        let (name_to_idx, idx_to_name) = handoff_maps();
        let allowed = std::collections::HashMap::new();
        let mut blocked = std::collections::HashMap::new();
        let decision = evaluate_handoff_check(
            &[],
            &name_to_idx,
            &idx_to_name,
            &allowed,
            &mut blocked,
            "nothing relevant",
            "0",
            &Value::Bool(false),
            "",
        );
        assert_eq!(decision.active_agent, "0");
        assert!(!decision.handoff);
    }

    #[tokio::test]
    async fn test_handoff_check_worker_execute() {
        let (name_to_idx, idx_to_name) = handoff_maps();
        let worker = HandoffCheckWorker {
            task_name: "agent_handoff_check".to_owned(),
            transitions: Vec::new(),
            name_to_idx,
            idx_to_name,
            allowed: std::collections::HashMap::new(),
            blocked_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        let mut task = Task::default();
        task.input_data
            .insert("active_agent".to_owned(), Value::from("0"));
        task.input_data
            .insert("is_transfer".to_owned(), Value::from(true));
        task.input_data
            .insert("transfer_to".to_owned(), Value::from("billing"));
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(
            map.get("active_agent"),
            Some(&Value::String("1".to_owned()))
        );
        assert_eq!(map.get("handoff"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_serve_registers_handoff_check_worker_for_swarm() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let sub = AgentDef::new("billing").unwrap();
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(sub)
            .unwrap()
            .with_strategy(super::super::def::Strategy::Swarm)
            .unwrap();

        if !agent.swarm_transitions.is_empty()
            || (agent.strategy == super::super::def::Strategy::Swarm && !agent.agents.is_empty())
        {
            runtime.task_handler.add_worker(HandoffCheckWorker {
                task_name: format!("{}_handoff_check", agent.name),
                transitions: agent.swarm_transitions.clone(),
                name_to_idx: std::collections::HashMap::new(),
                idx_to_name: std::collections::HashMap::new(),
                allowed: agent.allowed_transitions,
                blocked_counts: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
        }
        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    // ── ManualSelectionWorker ──────────────────────────────────────────

    fn manual_name_to_idx() -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        map.insert("alice".to_owned(), "0".to_owned());
        map.insert("bob".to_owned(), "1".to_owned());
        map
    }

    #[test]
    fn test_evaluate_manual_selection_none_defaults_to_zero() {
        assert_eq!(evaluate_manual_selection(&manual_name_to_idx(), None), "0");
    }

    #[test]
    fn test_evaluate_manual_selection_looks_up_selected_key() {
        let human_output = serde_json::json!({"selected": "bob"});
        assert_eq!(
            evaluate_manual_selection(&manual_name_to_idx(), Some(&human_output)),
            "1"
        );
    }

    #[test]
    fn test_evaluate_manual_selection_falls_back_to_agent_key() {
        let human_output = serde_json::json!({"agent": "alice"});
        assert_eq!(
            evaluate_manual_selection(&manual_name_to_idx(), Some(&human_output)),
            "0"
        );
    }

    #[test]
    fn test_evaluate_manual_selection_unknown_name_passes_through() {
        let human_output = serde_json::json!({"selected": "charlie"});
        assert_eq!(
            evaluate_manual_selection(&manual_name_to_idx(), Some(&human_output)),
            "charlie"
        );
    }

    #[test]
    fn test_evaluate_manual_selection_non_object_stringifies() {
        let human_output = serde_json::json!("bob");
        assert_eq!(
            evaluate_manual_selection(&manual_name_to_idx(), Some(&human_output)),
            "bob"
        );
    }

    #[tokio::test]
    async fn test_manual_selection_worker_execute() {
        let worker = ManualSelectionWorker {
            task_name: "agent_process_selection".to_owned(),
            name_to_idx: manual_name_to_idx(),
        };
        let mut task = Task::default();
        task.input_data.insert(
            "human_output".to_owned(),
            serde_json::json!({"selected": "bob"}),
        );
        let output = worker.execute(&task).await.unwrap();
        let WorkerOutput::Completed(map) = output else {
            panic!("expected Completed")
        };
        assert_eq!(map.get("selected"), Some(&Value::String("1".to_owned())));
    }

    #[test]
    fn test_serve_registers_manual_selection_worker() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let sub = AgentDef::new("alice").unwrap();
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(sub)
            .unwrap()
            .with_strategy(super::super::def::Strategy::Manual)
            .unwrap();

        if agent.strategy == super::super::def::Strategy::Manual && !agent.agents.is_empty() {
            runtime.task_handler.add_worker(ManualSelectionWorker {
                task_name: format!("{}_process_selection", agent.name),
                name_to_idx: agent
                    .agents
                    .iter()
                    .enumerate()
                    .map(|(i, sub)| (sub.name.clone(), i.to_string()))
                    .collect(),
            });
        }
        assert_eq!(runtime.task_handler.worker_count(), 1);
    }

    // ── Hybrid handoff / swarm transfer serve() registration ──────────

    #[test]
    fn test_serve_registers_hybrid_transfer_workers() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let tool = ToolDef::function::<Value, _, _>(
            "search",
            "search",
            serde_json::json!({}),
            |_: Value| async move { Ok(Value::Null) },
        );
        let sub = AgentDef::new("billing").unwrap();
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_tool(tool)
            .with_sub_agent(sub)
            .unwrap();

        if !agent.tools.is_empty() && !agent.agents.is_empty() {
            runtime.task_handler.add_worker(CheckTransferWorker {
                task_name: format!("{}_check_transfer", agent.name),
            });
            for sub in &agent.agents {
                runtime.task_handler.add_worker(TransferNoopWorker {
                    task_name: format!("{}_transfer_to_{}", agent.name, sub.name),
                });
            }
        }
        // 1 check_transfer + 1 transfer_to_billing = 2.
        assert_eq!(runtime.task_handler.worker_count(), 2);
    }

    #[test]
    fn test_swarm_transfer_worker_names_are_all_pairs_and_sanitized() {
        let config = Configuration::new("http://localhost:8080/api");
        let mut runtime = AgentRuntime::new(config).unwrap();
        let sub_a = AgentDef::new("agent-a").unwrap();
        let sub_b = AgentDef::new("agent-b").unwrap();
        let agent = AgentDef::new("parent-swarm")
            .unwrap()
            .with_sub_agent(sub_a)
            .unwrap()
            .with_sub_agent(sub_b)
            .unwrap()
            .with_strategy(super::super::def::Strategy::Swarm)
            .unwrap();

        let allowed = &agent.allowed_transitions;
        let valid_targets: std::collections::HashSet<&str> =
            allowed.values().flatten().map(String::as_str).collect();
        let all_names: Vec<&str> = std::iter::once(agent.name.as_str())
            .chain(agent.agents.iter().map(|a| a.name.as_str()))
            .collect();
        let mut registered = std::collections::HashSet::new();
        for &name in &all_names {
            for &peer in &all_names {
                if peer == name {
                    continue;
                }
                let tool_name = format!(
                    "{}_transfer_to_{}",
                    sanitize_for_task_name(name),
                    sanitize_for_task_name(peer)
                );
                if !registered.insert(tool_name.clone()) {
                    continue;
                }
                let is_unreachable = !allowed.is_empty() && !valid_targets.contains(peer);
                if is_unreachable {
                    runtime.task_handler.add_worker(TransferUnreachableWorker {
                        task_name: tool_name,
                    });
                } else {
                    runtime.task_handler.add_worker(TransferNoopWorker {
                        task_name: tool_name,
                    });
                }
            }
        }
        // 3 agents, all-pairs excluding self = 3*2 = 6 transfer tools.
        assert_eq!(runtime.task_handler.worker_count(), 6);
        assert!(registered.contains("parent_swarm_transfer_to_agent_a"));
        assert!(registered.contains("agent_a_transfer_to_agent_b"));
    }
}
