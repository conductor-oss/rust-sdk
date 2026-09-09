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
mod credentials;
mod def;
mod guardrail;
mod memory;
mod serializer;
mod swarm;
mod termination;
mod tool;

pub use callback::{CallbackContext, CallbackHandler};
pub use credentials::Credentials;
pub use def::{AgentDef, RunSettings, Strategy};
pub use guardrail::{
    Guardrail, GuardrailCheck, GuardrailResult, LlmGuardrail, OnFail, Position, RegexGuardrail,
    RegexMode,
};
pub use memory::{ConversationMemory, Message, MessageRole, ToolCall};
pub use serializer::AgentConfigSerializer;
pub use swarm::{SwarmConditionFn, SwarmContext, SwarmTransition};
pub use termination::TerminationCondition;
pub use tool::{ToolDef, ToolHandler, ToolType};
