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
use serde_json::Value;
use std::time::Duration;

use crate::client::AgentClient;
use crate::configuration::Configuration;
use crate::error::{ConductorError, Result};
use crate::http::ApiClient;
use crate::models::Task;
use crate::worker::{TaskHandler, Worker, WorkerOutput};

use super::credentials::Credentials;
use super::def::AgentDef;
use super::serializer::AgentConfigSerializer;
use super::tool::{ToolDef, ToolHandler};

/// Interval between `get_status` polls in [`AgentRuntime::run`] / [`AgentHandle::join`].
///
/// No server-provided guidance on an ideal interval exists yet (this crate's [`AgentClient`] has
/// no push/streaming transport — see its module docs), so this is a fixed, conservative default
/// rather than an exponential backoff: agent turns are LLM-latency-bound (seconds), not
/// sub-second operations, so a fixed 1s poll doesn't meaningfully add to end-to-end latency.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Lifecycle status of an agent execution.
///
/// Best-effort mapping over the raw `status` string [`AgentClient::get_status`] /
/// [`AgentClient::get_execution`] return as untyped [`Value`] — this crate has no formalized
/// `AgentExecution` schema yet (see `agent_client.rs`'s module docs), so [`AgentStatus::parse`]
/// accepts the reasonable set of spellings a Conductor workflow-backed execution can report
/// rather than asserting one canonical wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Paused,
    WaitingForInput,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl AgentStatus {
    /// Parse a status out of a `get_status`/`get_execution` response body. Looks for a top-level
    /// `status` string field (falling back to `agentStatus`, `workflowStatus` for whichever key
    /// a given server response uses), matched case-insensitively against known spellings.
    pub fn parse(value: &Value) -> Result<Self> {
        let raw = value
            .get("status")
            .or_else(|| value.get("agentStatus"))
            .or_else(|| value.get("workflowStatus"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ConductorError::agent(format!(
                    "agent execution response has no recognizable status field: {value}"
                ))
            })?;

        match raw.to_ascii_uppercase().replace('-', "_").as_str() {
            "RUNNING" | "IN_PROGRESS" => Ok(AgentStatus::Running),
            "PAUSED" => Ok(AgentStatus::Paused),
            "WAITING_FOR_INPUT" | "WAITING" | "HUMAN_IN_LOOP" => Ok(AgentStatus::WaitingForInput),
            "COMPLETED" | "SUCCESS" => Ok(AgentStatus::Completed),
            "FAILED" | "FAILED_WITH_TERMINAL_ERROR" | "ERROR" => Ok(AgentStatus::Failed),
            "CANCELLED" | "CANCELED" | "TERMINATED" => Ok(AgentStatus::Cancelled),
            "TIMED_OUT" | "TIMEOUT" => Ok(AgentStatus::TimedOut),
            other => Err(ConductorError::agent(format!(
                "unrecognized agent execution status: '{other}'"
            ))),
        }
    }

    /// True if this status will never change again — i.e. safe to stop polling.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentStatus::Completed
                | AgentStatus::Failed
                | AgentStatus::Cancelled
                | AgentStatus::TimedOut
        )
    }
}

/// The outcome of a completed (terminal) agent execution, returned by [`AgentRuntime::run`] /
/// [`AgentHandle::join`].
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub execution_id: String,
    pub status: AgentStatus,
    /// The full `get_execution` response body at the moment the execution reached a terminal
    /// status — kept as raw [`Value`] for the same reason [`AgentClient`] is untyped end-to-end
    /// (no `AgentExecution` model exists in this crate yet).
    pub output: Value,
}

/// A handle to a started (not-yet-necessarily-finished) agent execution.
///
/// Returned by [`AgentRuntime::start`]; wraps a cloned [`AgentClient`] (cheap — it's itself a
/// thin `Arc`-free wrapper over [`ApiClient`], which *is* internally `Arc`-backed) plus the
/// `execution_id` [`AgentClient::start_agent`]'s response carried. Every method here targets that
/// one `execution_id` — for `Handoff`/`Sequential`/`Parallel` strategies where a pending
/// human-in-the-loop task lives in a nested sub-execution with its own id, call
/// [`AgentClient::respond`] directly against that nested id instead of through this handle (see
/// `docs/agents/rust-sdk-design.md`'s note on this).
///
/// Does not expose `stream()`: SSE event streaming needs a streaming HTTP transport
/// [`AgentClient`] doesn't have yet (see its module docs) — deferred to the same follow-up PR.
#[derive(Clone)]
pub struct AgentHandle {
    agent_client: AgentClient,
    execution_id: String,
}

impl AgentHandle {
    fn new(agent_client: AgentClient, execution_id: String) -> Self {
        Self {
            agent_client,
            execution_id,
        }
    }

    /// The execution id this handle targets.
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }

    /// Fetch the current status without blocking.
    pub async fn status(&self) -> Result<AgentStatus> {
        let value = self.agent_client.get_status(&self.execution_id).await?;
        AgentStatus::parse(&value)
    }

    /// Block until the execution reaches a terminal status, polling
    /// [`AgentClient::get_status`] every [`POLL_INTERVAL`], then return the [`AgentResult`] built
    /// from the final [`AgentClient::get_execution`] response.
    pub async fn join(&self) -> Result<AgentResult> {
        loop {
            let status = self.status().await?;
            if status.is_terminal() {
                let output = self.agent_client.get_execution(&self.execution_id).await?;
                return Ok(AgentResult {
                    execution_id: self.execution_id.clone(),
                    status,
                    output,
                });
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Respond to a pending human-in-the-loop request on this execution.
    pub async fn respond(&self, body: &Value) -> Result<()> {
        self.agent_client.respond(&self.execution_id, body).await
    }

    /// Approve a pending human-in-the-loop request (`respond` with `{"approved": true}`).
    pub async fn approve(&self) -> Result<()> {
        self.respond(&serde_json::json!({ "approved": true })).await
    }

    /// Reject a pending human-in-the-loop request (`respond` with `{"approved": false}`).
    pub async fn reject(&self) -> Result<()> {
        self.respond(&serde_json::json!({ "approved": false }))
            .await
    }

    /// Stop the running execution.
    pub async fn stop(&self) -> Result<()> {
        self.agent_client.stop(&self.execution_id).await
    }
}

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
        let input = serde_json::to_value(&task.input_data)?;
        let output = (self.handler)(input, credentials).await?;
        Ok(WorkerOutput::completed_with_result(output))
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
    pub fn new(config: Configuration) -> Result<Self> {
        let api_client = ApiClient::new(config.clone())?;
        let agent_client = AgentClient::new(api_client);
        let task_handler = TaskHandler::new(config)?;

        Ok(Self {
            agent_client,
            task_handler,
        })
    }

    /// Compile an [`AgentDef`] into a Conductor workflow, without deploying it.
    ///
    /// Serializes `agent` via [`AgentConfigSerializer::serialize`] (producing the exact
    /// `agentConfig` payload shape [`super::serializer`]'s contract tests assert on), then POSTs
    /// it via [`AgentClient::compile_agent`].
    pub async fn compile(&self, agent: &AgentDef) -> Result<Value> {
        let payload = AgentConfigSerializer::serialize(agent);
        self.agent_client.compile_agent(&payload).await
    }

    /// Compile and register an [`AgentDef`] as a Conductor workflow.
    pub async fn deploy(&self, agent: &AgentDef) -> Result<Value> {
        let payload = AgentConfigSerializer::serialize(agent);
        self.agent_client.deploy_agent(&payload).await
    }

    /// Start an agent execution without blocking for completion.
    ///
    /// Serializes `agent`, bundles it with `input` into the `/agent/start` request body, and
    /// wraps the `executionId` the response carries in an [`AgentHandle`].
    pub async fn start(&self, agent: &AgentDef, input: Value) -> Result<AgentHandle> {
        let payload = serde_json::json!({
            "agentConfig": AgentConfigSerializer::serialize(agent),
            "input": input,
        });
        let response = self.agent_client.start_agent(&payload).await?;
        let execution_id = response
            .get("executionId")
            .or_else(|| response.get("execution_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ConductorError::agent(format!(
                    "start_agent response has no executionId: {response}"
                ))
            })?
            .to_string();

        Ok(AgentHandle::new(self.agent_client.clone(), execution_id))
    }

    /// Start an agent execution and block until it reaches a terminal status.
    ///
    /// Equivalent to `self.start(agent, input).await?.join().await`.
    pub async fn run(&self, agent: &AgentDef, input: Value) -> Result<AgentResult> {
        self.start(agent, input).await?.join().await
    }

    /// Register every locally-invoked tool on `agent` (i.e. every [`ToolDef`] whose `handler` is
    /// `Some`) as a worker on this runtime's [`TaskHandler`], then start polling.
    ///
    /// Server-side tools (`http`/`mcp`/`agent_tool`/`human`) need no local worker and are
    /// skipped. Blocks for as long as the underlying [`TaskHandler::start`] keeps its runners
    /// alive; call [`AgentRuntime::shutdown`] (from another task) to stop them.
    pub async fn serve(&mut self, agent: &AgentDef) -> Result<()> {
        for tool in &agent.tools {
            if let Some(worker) = ToolWorker::from_tool_def(tool) {
                self.task_handler.add_worker(worker);
            }
        }
        self.task_handler.start().await
    }

    /// Gracefully stop every worker [`AgentRuntime::serve`] registered.
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
        assert!(runtime.is_ok());
    }

    /// `compile()` (and `deploy()`/`start()`) build their outgoing payload via
    /// `AgentConfigSerializer::serialize` before ever touching `AgentClient` — this asserts on
    /// that exact shape, since actually calling `compile()` would require a live server.
    #[test]
    fn test_compile_payload_matches_agent_config_serializer_shape() {
        let agent = AgentDef::new("compiler_test")
            .unwrap()
            .with_model("openai/gpt-4o");

        let payload = AgentConfigSerializer::serialize(&agent);
        let obj = payload.as_object().unwrap();

        assert_eq!(
            obj.get("name"),
            Some(&Value::String("compiler_test".to_string()))
        );
        assert_eq!(
            obj.get("model"),
            Some(&Value::String("openai/gpt-4o".to_string()))
        );
        assert_eq!(obj.get("external"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_agent_status_parse_and_is_terminal() {
        assert_eq!(
            AgentStatus::parse(&serde_json::json!({"status": "RUNNING"})).unwrap(),
            AgentStatus::Running
        );
        assert!(!AgentStatus::Running.is_terminal());

        assert_eq!(
            AgentStatus::parse(&serde_json::json!({"status": "completed"})).unwrap(),
            AgentStatus::Completed
        );
        assert!(AgentStatus::Completed.is_terminal());

        assert!(AgentStatus::parse(&serde_json::json!({"status": "bogus"})).is_err());
        assert!(AgentStatus::parse(&serde_json::json!({})).is_err());
    }

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
                let token = creds.get("API_KEY").unwrap().to_string();
                async move { Ok(Value::from(format!("{token}:{}", args.n * 2))) }
            },
        )
        .with_credentials(vec!["API_KEY".to_string()]);

        let worker = ToolWorker::from_tool_def(&tool).expect("tool has a local handler");
        assert_eq!(worker.task_definition_name(), "double_with_token");
        assert_eq!(worker.declared_credentials(), vec!["API_KEY".to_string()]);

        let mut task = Task {
            runtime_metadata: HashMap::from([("API_KEY".to_string(), "secret".to_string())]),
            ..Default::default()
        };
        task.input_data.insert("n".to_string(), Value::from(21));

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
}
