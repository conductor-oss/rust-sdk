// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Declarative agent definitions and tool declarations ("Agents" feature, `agents` Cargo
//! feature). Same `agentConfig` wire format as python-sdk's `conductor.ai.agents` for the
//! subset of fields this crate currently models — see `docs/agents/` in the repo root for the
//! full design and what's deferred to follow-up PRs.
//!
//! This module has no dependency on a running `AgentRuntime` (that type doesn't exist yet in
//! this crate) — everything here is pure data + serialization, usable standalone to build and
//! inspect `agentConfig` JSON.

mod callback;
#[cfg(feature = "claude-agent-sdk")]
mod claude_agent_sdk;
mod cli_config;
mod code_execution_config;
mod code_executor;
mod credentials;
mod def;
mod framework;
#[cfg(feature = "openai-adapter")]
mod framework_openai;
mod graph;
mod guardrail;
mod handle;
#[cfg(feature = "jupyter")]
mod jupyter_executor;
mod liveness;
mod mcp_discovery;
mod memory;
mod ocg;
mod plan;
mod result;
mod runtime;
mod semantic_memory;
mod serializer;
mod skill;
mod stream;
mod swarm;
mod termination;
mod testing;
mod tool;
mod tracing;

pub use callback::{CallbackContext, CallbackHandler};
#[cfg(feature = "claude-agent-sdk")]
pub use claude_agent_sdk::{
    push_event_nonblocking, update_task_progress_nonblocking, ClaudeAgentSdkOptions,
    ClaudeAgentSdkQuery, ClaudeAgentSdkStream, ProgressMetadata, ProgressThrottle,
    PROGRESS_UPDATE_INTERVAL,
};
pub use cli_config::CliConfig;
pub use code_execution_config::{CodeExecutionConfig, CommandValidator, ConfiguredExecutor};
pub use code_executor::{
    CodeExecutor, DockerCodeExecutor, ExecutionResult, LocalCodeExecutor, ServerlessCodeExecutor,
};
pub use credentials::Credentials;
pub use def::{
    AgentDef, GateCondition, GateHandler, PrefillToolCall, RunSettings, StopWhenHandler, Strategy,
    TextGate,
};
pub use framework::FrameworkAgent;
#[cfg(feature = "openai-adapter")]
pub use framework_openai::OpenAiAgent;
pub use graph::{
    ConditionalGraphEdge, GraphAgentDef, GraphConditionFn, GraphContext, GraphEdge, GraphNode,
};
pub use guardrail::{
    FunctionGuardrail, Guardrail, GuardrailCheck, GuardrailResult, LlmGuardrail, OnFail, Position,
    RegexGuardrail, RegexMode,
};
pub use handle::AgentHandle;
#[cfg(feature = "jupyter")]
pub use jupyter_executor::JupyterCodeExecutor;
pub use liveness::StallPolicy;
pub use mcp_discovery::{
    clear_mcp_discovery_cache, discover_mcp_tools, expand_mcp_tool_def, DiscoveredMcpTool,
};
pub use memory::{ConversationMemory, Message, MessageRole, ToolCall};
pub use ocg::{ocg_agent, ocg_tools, OcgAgentOptions, OcgToolSelection, OCG_SYSTEM_PROMPT};
pub use plan::{
    plan_execute, Action, Context, Generate, Op, OpBody, Plan, PlanExecuteOptions, Ref, Step,
    Validation,
};
pub use result::{AgentResult, AgentStatus, ToolCallRecord};
pub use runtime::AgentRuntime;
pub use semantic_memory::{InMemoryStore, MemoryEntry, MemoryStore, SemanticMemory};
pub use serializer::AgentConfigSerializer;
pub use skill::{
    create_skill_workers, format_prompt_with_params, load_skill, load_skills, SkillAgent,
    SkillOptions,
};
pub use stream::{AgentEvent, AgentStream};
pub use swarm::{SwarmConditionFn, SwarmContext, SwarmTransition};
pub use termination::TerminationCondition;
pub use testing::{expect, mock_run, Expect, ScriptedEvent};
pub use tool::{ToolContext, ToolDef, ToolHandler, ToolType};
pub use tracing::{
    agent_run_span, compile_span, handoff_span, is_tracing_enabled, llm_call_span,
    record_token_usage, tool_call_span, traced_agent_run, traced_tool_call,
};
