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
mod credentials;
mod def;
mod framework;
#[cfg(feature = "openai-adapter")]
mod framework_openai;
mod guardrail;
mod handle;
mod memory;
mod result;
mod runtime;
mod serializer;
mod stream;
mod swarm;
mod termination;
mod tool;

pub use callback::{CallbackContext, CallbackHandler};
#[cfg(feature = "claude-agent-sdk")]
pub use claude_agent_sdk::{ClaudeAgentSdkOptions, ClaudeAgentSdkQuery, ClaudeAgentSdkStream};
pub use credentials::Credentials;
pub use def::{AgentDef, RunSettings, Strategy};
pub use framework::FrameworkAgent;
#[cfg(feature = "openai-adapter")]
pub use framework_openai::OpenAiAgent;
pub use guardrail::{
    Guardrail, GuardrailCheck, GuardrailResult, LlmGuardrail, OnFail, Position, RegexGuardrail,
    RegexMode,
};
pub use handle::AgentHandle;
pub use memory::{ConversationMemory, Message, MessageRole, ToolCall};
pub use result::{AgentExecutionState, AgentResult, AgentStatus};
pub use runtime::AgentRuntime;
pub use serializer::AgentConfigSerializer;
pub use stream::{AgentEvent, AgentStream};
pub use swarm::{SwarmConditionFn, SwarmContext, SwarmTransition};
pub use termination::TerminationCondition;
pub use tool::{ToolDef, ToolHandler, ToolType};
