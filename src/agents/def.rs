// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::callback::CallbackHandler;
use super::guardrail::Guardrail;
use super::memory::ConversationMemory;
use super::swarm::SwarmTransition;
use super::termination::TerminationCondition;
use super::tool::ToolDef;

/// Multi-agent orchestration strategy.
///
/// Kept as the complete 9-variant python-sdk enum for wire compatibility. This SDK version
/// supports *constructing* agents with `Handoff`, `Sequential`, `Parallel`, `Router` (requires
/// a router sub-agent, see [`AgentDef::with_router`]), `RoundRobin`, `Random`, `Swarm` (requires
/// `swarm_transitions`, see [`AgentDef::with_swarm_transition`]), `PlanExecute` (requires a
/// planner, see [`AgentDef::with_planner`]), and `Manual`.
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
    /// Requires a router sub-agent set via [`AgentDef::with_router`].
    Router,
    RoundRobin,
    Random,
    /// Rule-based agent-to-agent transitions; see `AgentDef::swarm_transitions` /
    /// [`AgentDef::with_swarm_transition`]. Matching python-sdk's `Agent.__init__` (which has
    /// no analogous check for `strategy="swarm"`), zero transitions is accepted too.
    Swarm,
    Manual,
    /// Requires a `planner` composition field, set via [`AgentDef::with_planner`]; `fallback`
    /// is optional.
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
/// `Guardrail`, `TerminationCondition`, `SwarmTransition`, `ConversationMemory`, `Router`
/// (agent-based only — see [`AgentDef::router`]), and `PlanExecute`'s composition fields are
/// all fully wired. `CallbackHandler` is registered (`callbacks`) but deliberately not
/// serialized — see `serializer.rs`. See `docs/agents/` for the full exclusion set and
/// rationale on anything still deferred.
///
/// Construct via [`AgentDef::new`], compose with consuming `with_*` builders — matching
/// [`TaskDef`](crate::models::TaskDef)'s pattern exactly (100% `fn with_x(mut self, ...) -> Self`,
/// no `&mut self` builders) — and serialize with [`AgentConfigSerializer`](super::AgentConfigSerializer).
///
/// `Debug` is implemented by hand rather than derived: `callbacks` holds `Arc<dyn
/// CallbackHandler>` trait objects, and `CallbackHandler` doesn't require `Debug` on its
/// implementors (adding that bound would force every caller-provided handler — including
/// closures-in-disguise and simple test mocks — to also implement `Debug` just to be
/// registered, which is a bigger ask than this field's debug-printability is worth). See the
/// manual `impl Debug for AgentDef` below.
#[derive(Clone)]
pub struct AgentDef {
    pub name: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub instructions: Option<String>,
    pub tools: Vec<ToolDef>,
    pub guardrails: Vec<Guardrail>,
    pub agents: Vec<AgentDef>,
    /// Sub-agent that picks which agent runs each turn, used when `strategy =
    /// Strategy::Router`.
    ///
    /// Narrowed from python-sdk's `router: Optional[Union[Agent, Callable[..., Any]]]`: Rust
    /// has no clean equivalent of an arbitrary callable that also serializes to JSON as part
    /// of an `AgentDef` tree, so this field only models the agent-based form — a full
    /// sub-agent whose job is to select the next agent. A callable-based router is out of
    /// scope for this SDK version.
    pub router: Option<Box<AgentDef>>,
    /// Rule-based agent-to-agent transitions, used when `strategy = Strategy::Swarm` (python's
    /// `Agent(handoffs=[...])`).
    pub swarm_transitions: Vec<SwarmTransition>,
    pub strategy: Strategy,
    pub max_turns: u32,
    pub max_tokens: Option<u32>,
    pub timeout_seconds: u64,
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<String>,
    pub credentials: Vec<String>,
    pub required_tools: Vec<String>,
    pub context_window_budget: Option<u32>,
    pub termination: Option<TerminationCondition>,
    pub memory: Option<ConversationMemory>,
    /// PLAN_EXECUTE planner sub-agent — produces the JSON plan the parent executes. Required
    /// when `strategy` is [`Strategy::PlanExecute`]; see [`AgentDef::with_strategy`].
    pub planner: Option<Box<AgentDef>>,
    /// PLAN_EXECUTE fallback sub-agent, invoked when the planner's plan fails mid-execution.
    /// Optional — PLAN_EXECUTE works without one.
    pub fallback: Option<Box<AgentDef>>,
    /// Turn cap applied to `fallback` once it's invoked.
    pub fallback_max_turns: Option<u32>,
    /// Reference text appended to the PLAN_EXECUTE planner's prompt as a
    /// `## Reference Context` block on every planner invocation.
    ///
    /// python-sdk's `planner_context` accepts a richer `Context` dataclass — exactly one of
    /// `text` or `url` (the latter HTTP-fetched at every planner run, with optional
    /// `headers`/`required`/`max_bytes`) — see
    /// `python-sdk/src/conductor/ai/agents/plans.py::Context`. It also accepts bare `str`,
    /// which it normalises to `Context(text=...)`.
    ///
    /// This crate models only that bare-string shape for now: a `PlannerContextEntry` type
    /// rich enough to cover the URL case would need to be a new public type re-exported from
    /// `agents::mod`, which is out of scope here. `Vec<String>` covers the common "inline
    /// reference text" case exactly like python's `str` shorthand; URL-backed entries are a
    /// follow-up once a richer type can be exported.
    pub planner_context: Vec<String>,
    /// Whether to run a final LLM synthesis step after execution completes. Defaults to
    /// `true` (matches python-sdk); only serialized on the wire when explicitly `false`.
    pub synthesize: bool,
    pub metadata: HashMap<String, Value>,
    /// Lifecycle hooks registered by the caller (see [`CallbackHandler`]). `Arc`, not `Box`,
    /// because a single handler instance (e.g. one metrics sink) may reasonably be shared across
    /// multiple agents or multiple hook positions rather than constructed fresh each time —
    /// matching how [`ToolHandler`](super::tool::ToolHandler) is held for the same reason.
    /// Deliberately **not** serialized — see `serializer.rs`'s `serialize_agent` for why.
    pub callbacks: Vec<Arc<dyn CallbackHandler>>,
}

impl std::fmt::Debug for AgentDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDef")
            .field("name", &self.name)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("instructions", &self.instructions)
            .field("tools", &self.tools)
            .field("guardrails", &self.guardrails)
            .field("agents", &self.agents)
            .field("strategy", &self.strategy)
            .field("max_turns", &self.max_turns)
            .field("max_tokens", &self.max_tokens)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("temperature", &self.temperature)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("credentials", &self.credentials)
            .field("required_tools", &self.required_tools)
            .field("context_window_budget", &self.context_window_budget)
            .field("termination", &self.termination)
            .field("metadata", &self.metadata)
            .field("callbacks", &format!("<{} handlers>", self.callbacks.len()))
            .finish()
    }
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
            guardrails: Vec::new(),
            agents: Vec::new(),
            router: None,
            swarm_transitions: Vec::new(),
            strategy: Strategy::default(),
            max_turns: 25,
            max_tokens: None,
            timeout_seconds: 0,
            temperature: None,
            reasoning_effort: None,
            credentials: Vec::new(),
            required_tools: Vec::new(),
            context_window_budget: None,
            termination: None,
            memory: None,
            planner: None,
            fallback: None,
            fallback_max_turns: None,
            planner_context: Vec::new(),
            synthesize: true,
            metadata: HashMap::new(),
            callbacks: Vec::new(),
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

    pub fn with_guardrail(mut self, guardrail: Guardrail) -> Self {
        self.guardrails.push(guardrail);
        self
    }

    pub fn with_guardrails(mut self, guardrails: impl IntoIterator<Item = Guardrail>) -> Self {
        self.guardrails.extend(guardrails);
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

    /// Set the router sub-agent used when `strategy = Strategy::Router`.
    ///
    /// Call this *before* [`AgentDef::with_strategy`]`(Strategy::Router)`: that call validates
    /// the router requirement against this field's current state at the time it's called, so
    /// setting the router afterwards does not retroactively satisfy an already-failed
    /// `with_strategy` call.
    pub fn with_router(mut self, router: AgentDef) -> Self {
        self.router = Some(Box::new(router));
        self
    }

    /// Add a rule-based agent-to-agent transition, used under `Strategy::Swarm` (python's
    /// `Agent(handoffs=[...])`). Accumulates like [`AgentDef::with_guardrail`] — call once per
    /// transition.
    pub fn with_swarm_transition(mut self, transition: SwarmTransition) -> Self {
        self.swarm_transitions.push(transition);
        self
    }

    /// Set the orchestration strategy.
    ///
    /// `Router` and `PlanExecute` each validate a required composition field against its
    /// *current* state at the moment this method runs: [`AgentDef::with_router`] /
    /// [`AgentDef::with_planner`] must be called first in the builder chain, since an `Err`
    /// here consumes `self` and there's no way to retroactively satisfy the check afterwards.
    /// `Swarm` has no such requirement — matching python-sdk's `Agent.__init__` (which has no
    /// analogous check for `strategy="swarm"`), zero `swarm_transitions` is accepted too.
    pub fn with_strategy(mut self, strategy: Strategy) -> Result<Self> {
        if strategy == Strategy::Router && self.router.is_none() {
            return Err(ConductorError::agent(
                "strategy='router' requires a router argument: call with_router(...) before \
                 with_strategy(Strategy::Router)",
            ));
        }
        if strategy == Strategy::PlanExecute && self.planner.is_none() {
            return Err(ConductorError::agent(
                "strategy 'plan_execute' requires a planner: call `.with_planner(...)` before \
                 `.with_strategy(Strategy::PlanExecute)` (the planner agent produces the JSON \
                 plan the parent executes)",
            ));
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

    pub fn with_termination(mut self, termination: TerminationCondition) -> Self {
        self.termination = Some(termination);
        self
    }

    pub fn with_memory(mut self, memory: ConversationMemory) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Set the PLAN_EXECUTE planner sub-agent — the agent that produces the JSON plan the
    /// parent executes. Call this *before* `.with_strategy(Strategy::PlanExecute)`; see that
    /// method's validation.
    pub fn with_planner(mut self, planner: AgentDef) -> Self {
        self.planner = Some(Box::new(planner));
        self
    }

    /// Set the PLAN_EXECUTE fallback sub-agent, invoked when the planner's plan fails
    /// mid-execution. Optional — PLAN_EXECUTE works without one.
    pub fn with_fallback(mut self, fallback: AgentDef) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// Cap the number of turns the `fallback` agent gets once invoked.
    pub fn with_fallback_max_turns(mut self, turns: u32) -> Self {
        self.fallback_max_turns = Some(turns);
        self
    }

    /// Append one reference-text entry to the PLAN_EXECUTE planner's context. Matches
    /// python-sdk's bare-`str` shorthand for `Context(text=...)` — see the `planner_context`
    /// field's doc comment on [`AgentDef`] for why URL-backed entries aren't modeled here yet.
    pub fn with_planner_context(mut self, entry: impl Into<String>) -> Self {
        self.planner_context.push(entry.into());
        self
    }

    /// Bulk variant of [`with_planner_context`](Self::with_planner_context).
    pub fn with_planner_contexts(
        mut self,
        entries: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.planner_context
            .extend(entries.into_iter().map(Into::into));
        self
    }

    /// Toggle the final LLM synthesis step after execution completes. Defaults to `true`
    /// (matches python-sdk); only emitted on the wire when explicitly disabled.
    pub fn with_synthesize(mut self, synthesize: bool) -> Self {
        self.synthesize = synthesize;
        self
    }

    pub fn with_metadata_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Register a [`CallbackHandler`]. Multiple registrations accumulate and are expected to
    /// chain in registration order (see `callback.rs`'s module docs) once a dispatcher exists;
    /// this crate only stores them for now — see `serializer.rs` for why they aren't serialized.
    pub fn with_callback(mut self, callback: impl CallbackHandler + 'static) -> Self {
        self.callbacks.push(Arc::new(callback));
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
    use super::super::guardrail::RegexGuardrail;
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
    fn test_with_strategy_accepts_simple_variants() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.with_strategy(Strategy::Sequential).is_ok());
    }

    #[test]
    fn test_with_strategy_router_succeeds_with_router_set() {
        let router_agent = AgentDef::new("router_agent").unwrap();
        let agent = AgentDef::new("a")
            .unwrap()
            .with_router(router_agent)
            .with_strategy(Strategy::Router);
        assert!(agent.is_ok());
        assert_eq!(agent.unwrap().strategy, Strategy::Router);
    }

    #[test]
    fn test_with_strategy_router_fails_without_router_set() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.router.is_none());
        assert!(agent.with_strategy(Strategy::Router).is_err());
    }

    #[test]
    fn test_with_strategy_accepts_swarm_with_a_transition() {
        let agent =
            AgentDef::new("a")
                .unwrap()
                .with_swarm_transition(SwarmTransition::OnTextMention {
                    target: "b".into(),
                    text: "ACTIONABLE".into(),
                });
        assert!(agent.with_strategy(Strategy::Swarm).is_ok());
    }

    #[test]
    fn test_with_strategy_accepts_swarm_with_zero_transitions() {
        // Matches python-sdk's `Agent.__init__`, which has no check requiring
        // `handoffs` to be non-empty when `strategy="swarm"`.
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.with_strategy(Strategy::Swarm).is_ok());
    }

    #[test]
    fn test_with_swarm_transition_accumulates() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_swarm_transition(SwarmTransition::OnToolResult {
                target: "b".into(),
                tool_name: "check_order".into(),
                result_contains: None,
            })
            .with_swarm_transition(SwarmTransition::OnTextMention {
                target: "c".into(),
                text: "done".into(),
            });
        assert_eq!(agent.swarm_transitions.len(), 2);
        assert_eq!(agent.swarm_transitions[0].target(), "b");
        assert_eq!(agent.swarm_transitions[1].target(), "c");
    }

    #[test]
    fn test_with_strategy_plan_execute_requires_planner() {
        let agent = AgentDef::new("a").unwrap();
        // No planner set yet: rejected, matching python's "PLAN_EXECUTE requires planner=".
        assert!(agent.clone().with_strategy(Strategy::PlanExecute).is_err());

        // Planner set first (required builder-chain order): accepted.
        let planner = AgentDef::new("planner").unwrap();
        let agent_with_planner = agent.with_planner(planner);
        assert!(agent_with_planner
            .with_strategy(Strategy::PlanExecute)
            .is_ok());
    }

    #[test]
    fn test_plan_execute_composition_builders() {
        let default_agent = AgentDef::new("defaults").unwrap();
        assert!(default_agent.planner.is_none());
        assert!(default_agent.fallback.is_none());
        assert!(default_agent.fallback_max_turns.is_none());
        assert!(default_agent.planner_context.is_empty());
        assert!(default_agent.synthesize);

        let planner = AgentDef::new("planner").unwrap();
        let fallback = AgentDef::new("fallback_agent").unwrap();
        let agent = AgentDef::new("a")
            .unwrap()
            .with_planner(planner)
            .with_fallback(fallback)
            .with_fallback_max_turns(7)
            .with_planner_context("first")
            .with_planner_contexts(vec!["second", "third"])
            .with_synthesize(false);

        assert_eq!(agent.planner.as_ref().unwrap().name, "planner");
        assert_eq!(agent.fallback.as_ref().unwrap().name, "fallback_agent");
        assert_eq!(agent.fallback_max_turns, Some(7));
        assert_eq!(agent.planner_context, vec!["first", "second", "third"]);
        assert!(!agent.synthesize);
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

    #[test]
    fn test_with_guardrail_accumulates() {
        let checker_a = RegexGuardrail::new(["a"]).unwrap();
        let checker_b = RegexGuardrail::new(["b"]).unwrap();
        let agent = AgentDef::new("a")
            .unwrap()
            .with_guardrail(Guardrail::new("no_a", checker_a))
            .with_guardrail(Guardrail::new("no_b", checker_b));
        assert_eq!(agent.guardrails.len(), 2);
        assert_eq!(agent.guardrails[0].name, "no_a");
        assert_eq!(agent.guardrails[1].name, "no_b");
    }

    #[test]
    fn test_with_termination_sets_field() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.termination.is_none());

        let termination = TerminationCondition::max_message(10).unwrap();
        let agent = agent.with_termination(termination.clone());
        assert_eq!(agent.termination, Some(termination));
    }

    /// Minimal mock handler, adapted from `callback.rs`'s own `NoopHandler` test fixture:
    /// overrides nothing, so it exercises only "does this type implement `CallbackHandler` and
    /// can it be registered," not any particular hook behavior.
    struct MockCallbackHandler;

    #[async_trait::async_trait]
    impl super::super::callback::CallbackHandler for MockCallbackHandler {}

    #[test]
    fn test_with_callback_registers_handler() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.callbacks.is_empty());

        let agent = agent.with_callback(MockCallbackHandler);
        assert_eq!(agent.callbacks.len(), 1);
    }

    #[test]
    fn test_with_callback_accumulates() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_callback(MockCallbackHandler)
            .with_callback(MockCallbackHandler)
            .with_callback(MockCallbackHandler);
        assert_eq!(agent.callbacks.len(), 3);
    }

    #[test]
    fn test_debug_format_does_not_panic_with_callbacks() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_callback(MockCallbackHandler);
        let debug_str = format!("{:?}", agent);
        assert!(debug_str.contains("AgentDef"));
        assert!(debug_str.contains("1 handlers"));
    }

    #[test]
    fn test_with_memory_sets_field() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.memory.is_none());

        let mut memory = ConversationMemory::new();
        memory.add_user_message("hi");
        let agent = agent.with_memory(memory.clone());
        assert_eq!(agent.memory.map(|m| m.messages), Some(memory.messages));
    }
}
