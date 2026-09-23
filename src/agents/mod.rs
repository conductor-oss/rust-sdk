// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Declarative agent definitions and tool declarations (the `agents` Cargo feature).

mod callback;
mod credentials;
mod def;
mod guardrail;
mod handle;
mod memory;
mod plan;
mod result;
mod runtime;
mod schedule;
mod serializer;
mod stream;
mod swarm;
mod termination;
mod tool;
mod tracing;

pub use callback::{CallbackContext, CallbackHandler};
pub use credentials::Credentials;
pub use def::{
    AgentDef, GateCondition, GateHandler, PrefillToolCall, RunSettings, StopWhenHandler, Strategy,
    TextGate,
};
pub use guardrail::{
    FunctionGuardrail, Guardrail, GuardrailCheck, GuardrailResult, LlmGuardrail, OnFail, Position,
    RegexGuardrail, RegexMode,
};
pub use handle::AgentHandle;
pub use memory::{ConversationMemory, Message, MessageRole, ToolCall};
pub use plan::{
    plan_execute, Action, Context, Generate, Op, OpBody, Plan, PlanExecuteOptions, Ref, Step,
    Validation,
};
pub use result::{AgentResult, AgentStatus, ToolCallRecord};
pub use runtime::AgentRuntime;
pub use schedule::{list_schedules, wire_name, Schedule, ScheduleInfo};
pub use serializer::AgentConfigSerializer;
pub use stream::{AgentEvent, AgentStream};
pub use swarm::{SwarmConditionFn, SwarmContext, SwarmTransition};
pub use termination::TerminationCondition;
pub use tool::{ToolContext, ToolDef, ToolHandler, ToolType};
pub use tracing::{
    agent_run_span, compile_span, handoff_span, is_tracing_enabled, llm_call_span,
    record_token_usage, tool_call_span, traced_agent_run, traced_tool_call,
};
