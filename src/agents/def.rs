// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};
use serde_json::Value;
use std::collections::HashMap;

use super::tool::ToolDef;

/// Multi-agent orchestration strategy.
///
/// Kept as the complete 9-variant python-sdk enum for wire compatibility, even though this SDK
/// version only supports *constructing* agents with `Handoff`, `Sequential`, `Parallel`,
/// `RoundRobin`, `Random`, and `Manual` — see [`AgentDef::with_strategy`]. `Router`, `Swarm`,
/// and `PlanExecute` each require a composition field (`router` / `swarm_transitions` /
/// `planner`+`fallback`) that is deferred to a follow-up PR.
///
/// `Handoff` (LLM freely picks the next agent) and the swarm-only rule-based transition type
/// (python calls it `HandoffCondition`; the deferred Rust equivalent is named `SwarmTransition`,
/// never `HandoffCondition`) share vocabulary in python-sdk but are unrelated mechanisms. Keep
/// them named differently here — do not "fix" this back to match python.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strategy {
    /// LLM freely picks the next agent (default).
    #[default]
    Handoff,
    Sequential,
    Parallel,
    /// Requires a `router` composition field — not yet supported by this SDK version.
    Router,
    RoundRobin,
    Random,
    /// Requires `swarm_transitions` — not yet supported by this SDK version.
    Swarm,
    Manual,
    /// Requires `planner`/`fallback` — not yet supported by this SDK version.
    PlanExecute,
}

impl Strategy {
    /// Wire-format string, matching python-sdk's `Strategy(str, Enum)` values exactly.
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::Handoff => "handoff",
            Strategy::Sequential => "sequential",
            Strategy::Parallel => "parallel",
            Strategy::Router => "router",
            Strategy::RoundRobin => "round_robin",
            Strategy::Random => "random",
            Strategy::Swarm => "swarm",
            Strategy::Manual => "manual",
            Strategy::PlanExecute => "plan_execute",
        }
    }

    fn requires_deferred_composition(&self) -> bool {
        matches!(
            self,
            Strategy::Router | Strategy::Swarm | Strategy::PlanExecute
        )
    }
}

/// Per-call override struct reserved for the future `AgentRuntime::run` API.
///
/// Ports python-sdk's `RunSettings` as-is (already a small, clean, validation-free override
/// struct). **Not consumed by anything in this crate yet** — there is no `AgentRuntime::run` to
/// pass it to until that follow-up PR lands. Defined now because it has zero dependencies on any
/// deferred type, so shipping its shape now avoids a breaking rework later.
#[derive(Debug, Clone, Default)]
pub struct RunSettings {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<String>,
    pub thinking_budget_tokens: Option<u32>,
}

impl RunSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    pub fn with_thinking_budget_tokens(mut self, tokens: u32) -> Self {
        self.thinking_budget_tokens = Some(tokens);
        self
    }
}

/// Declarative agent definition.
///
/// Named `AgentDef` rather than python's `Agent` because this crate already has a
/// `TaskDef`/`Task` and `WorkflowDef`/`Workflow` split: `*Def` means "a definition you
/// serialize/register", the bare name means "a runtime instance". Python's `Agent` is
/// structurally a definition (it *is* what serializes to `agentConfig`), so `AgentDef` is the
/// name this crate's own convention implies — not an arbitrary rename.
///
/// Narrowed to the fields that are structurally meaningful without the deferred composition
/// types (`Guardrail`, `TerminationCondition`, `SwarmTransition`, `CallbackHandler`,
/// `ConversationMemory`, `Router`, `planner`/`fallback`). See `docs/agents/` for the full
/// exclusion set and rationale.
///
/// Construct via [`AgentDef::new`], compose with consuming `with_*` builders — matching
/// [`TaskDef`](crate::models::TaskDef)'s pattern exactly (100% `fn with_x(mut self, ...) -> Self`,
/// no `&mut self` builders) — and serialize with [`AgentConfigSerializer`](super::AgentConfigSerializer).
#[derive(Debug, Clone)]
pub struct AgentDef {
    pub name: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub instructions: Option<String>,
    pub tools: Vec<ToolDef>,
    pub agents: Vec<AgentDef>,
    pub strategy: Strategy,
    pub max_turns: u32,
    pub max_tokens: Option<u32>,
    pub timeout_seconds: u64,
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<String>,
    pub credentials: Vec<String>,
    pub required_tools: Vec<String>,
    pub context_window_budget: Option<u32>,
    pub metadata: HashMap<String, Value>,
}

impl AgentDef {
    /// Create a new agent definition. Validates `name` against `^[a-zA-Z_][a-zA-Z0-9_-]*$` up
    /// front (matches python-sdk's `Agent.__init__`) since the name doubles as the Conductor
    /// workflow name once compiled.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !is_valid_agent_name(&name) {
            return Err(ConductorError::agent(format!(
                "invalid agent name '{name}': must match ^[a-zA-Z_][a-zA-Z0-9_-]*$"
            )));
        }
        Ok(Self {
            name,
            model: None,
            base_url: None,
            instructions: None,
            tools: Vec::new(),
            agents: Vec::new(),
            strategy: Strategy::default(),
            max_turns: 25,
            max_tokens: None,
            timeout_seconds: 0,
            temperature: None,
            reasoning_effort: None,
            credentials: Vec::new(),
            required_tools: Vec::new(),
            context_window_budget: None,
            metadata: HashMap::new(),
        })
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn with_tool(mut self, tool: ToolDef) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn with_tools(mut self, tools: impl IntoIterator<Item = ToolDef>) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Add a sub-agent. Fails if its name collides with an already-added sub-agent's name
    /// (matches python's duplicate-name check in `Agent.__init__`, moved to construction time).
    pub fn with_sub_agent(mut self, agent: AgentDef) -> Result<Self> {
        if self.agents.iter().any(|a| a.name == agent.name) {
            return Err(ConductorError::agent(format!(
                "duplicate sub-agent name '{}': sub-agent names must be unique",
                agent.name
            )));
        }
        self.agents.push(agent);
        Ok(self)
    }

    /// Set the orchestration strategy. Rejects `Router`, `Swarm`, and `PlanExecute` — each
    /// requires a composition field this SDK version doesn't support yet; constructing one of
    /// them without its composition field would silently produce an incomplete `agentConfig`
    /// on the wire, so this fails closed at construction time instead.
    pub fn with_strategy(mut self, strategy: Strategy) -> Result<Self> {
        if strategy.requires_deferred_composition() {
            return Err(ConductorError::agent(format!(
                "strategy '{}' requires a composition field (router / swarm_transitions / \
                 planner+fallback) not yet supported by this SDK version; use Handoff, \
                 Sequential, Parallel, RoundRobin, Random, or Manual instead",
                strategy.as_str()
            )));
        }
        self.strategy = strategy;
        Ok(self)
    }

    /// Set the hard cap on conversation turns. Must be at least 1 (matches python's
    /// `max_turns < 1` check).
    pub fn with_max_turns(mut self, max_turns: u32) -> Result<Self> {
        if max_turns < 1 {
            return Err(ConductorError::agent("max_turns must be at least 1"));
        }
        self.max_turns = max_turns;
        Ok(self)
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Declared credential names — **inert in this SDK version**. Populates the field that will
    /// eventually flow into `TaskDef.runtime_metadata` at registration time; nothing resolves
    /// or delivers values yet. See `docs/agents/secrets-and-credentials.md`.
    pub fn with_credentials(mut self, credentials: Vec<String>) -> Self {
        self.credentials = credentials;
        self
    }

    pub fn with_required_tools(mut self, tools: Vec<String>) -> Self {
        self.required_tools = tools;
        self
    }

    pub fn with_context_window_budget(mut self, budget: u32) -> Self {
        self.context_window_budget = Some(budget);
        self
    }

    pub fn with_metadata_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

fn is_valid_agent_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_validates_name() {
        assert!(AgentDef::new("valid_name-1").is_ok());
        assert!(AgentDef::new("1invalid").is_err());
        assert!(AgentDef::new("in valid").is_err());
        assert!(AgentDef::new("").is_err());
    }

    #[test]
    fn test_defaults() {
        let agent = AgentDef::new("a").unwrap();
        assert_eq!(agent.max_turns, 25);
        assert_eq!(agent.timeout_seconds, 0);
        assert_eq!(agent.strategy, Strategy::Handoff);
        assert!(agent.model.is_none());
    }

    #[test]
    fn test_with_strategy_rejects_deferred_variants() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.clone().with_strategy(Strategy::Router).is_err());
        assert!(agent.clone().with_strategy(Strategy::Swarm).is_err());
        assert!(agent.clone().with_strategy(Strategy::PlanExecute).is_err());
        assert!(agent.with_strategy(Strategy::Sequential).is_ok());
    }

    #[test]
    fn test_sub_agent_name_uniqueness() {
        let child_a = AgentDef::new("child").unwrap();
        let child_b = AgentDef::new("child").unwrap();
        let parent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child_a)
            .unwrap();
        assert!(parent.with_sub_agent(child_b).is_err());
    }

    #[test]
    fn test_max_turns_validation() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.clone().with_max_turns(0).is_err());
        assert!(agent.with_max_turns(1).is_ok());
    }
}
