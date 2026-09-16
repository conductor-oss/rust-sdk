// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::{ConductorError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
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
    #[must_use]
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

/// Structured-output typing for an agent's final response.
///
/// Mirrors python-sdk's `Agent.output_type: Optional[type]`, serialized by
/// `config_serializer.py::_serialize_output_type` as
/// `{"schema": schema_from_pydantic(output_type), "className": output_type.__name__}`.
///
/// Narrowed for Rust: there's no runtime type to introspect the way python inspects a Pydantic
/// class, so the caller supplies both halves directly — the schema (generate one via
/// [`crate::schema::generate_schema::<T>(true)`](crate::schema::generate_schema) for a type `T`
/// implementing `JsonSchema`) and `T`'s name as `class_name`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputType {
    pub schema: Value,
    pub class_name: String,
}

/// A tool call to execute before the agent's first LLM turn, with its result injected into the
/// conversation as a `tool_call` + `tool_response` message pair.
///
/// Mirrors python-sdk's `agent.prefill_tools` entries, serialized by
/// `config_serializer.py` as `{"toolName": pt.tool_name, "arguments": pt.arguments}` — matches
/// the server's `PrefillToolCallConfig` DTO (`toolName: String`, `arguments: Map<String,
/// Object>`) field-for-field.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefillToolCall {
    pub tool_name: String,
    pub arguments: Value,
}

impl PrefillToolCall {
    pub fn new(tool_name: impl Into<String>, arguments: Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments,
        }
    }
}

/// A declarative gate condition for conditional sequential (`>>`) pipelines: stop the pipeline
/// after this agent if its output contains `text`.
///
/// Mirrors python-sdk's `gate.py::TextGate` dataclass exactly (`text`, `case_sensitive` default
/// `true`), serialized by `config_serializer.py::_serialize_gate` as `{"type": "text_contains",
/// "text": ..., "caseSensitive": ...}`. Per that module's own docstring, this is "compiled
/// entirely server-side (INLINE JavaScript) — no worker round-trip needed", which is why it's
/// safe to port on its own. See [`GateCondition`] for python's other `gate` shape (an arbitrary
/// callable), now also ported.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGate {
    pub text: String,
    pub case_sensitive: bool,
}

impl TextGate {
    /// New gate with `case_sensitive` defaulted to `true`, matching python's dataclass default.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            case_sensitive: true,
        }
    }

    #[must_use]
    pub fn case_insensitive(mut self) -> Self {
        self.case_sensitive = false;
        self
    }
}

/// A callable gate predicate: given `{"result": <agent output>}` (matching python-sdk's
/// `GateEntry.__call__`'s context dict), decide whether a conditional sequential (`>>`)
/// pipeline should continue past this agent.
///
/// `Result<bool>` rather than a bare `bool` for the same reason [`StopWhenHandler`] is —
/// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) treats an `Err` the same way
/// python's `GateEntry.__call__` treats an exception from the user callable: log it and fail
/// *open* (`{"decision": "continue"}`), not stop the pipeline over a broken predicate.
pub type GateHandler =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>> + Send + Sync>;

/// [`AgentDef::gate`]'s two possible shapes, matching python-sdk's `Agent.gate: Optional[Any]`
/// slot, which is either a [`TextGate`] instance or an arbitrary callable
/// (`config_serializer.py::_serialize_gate`'s `isinstance(gate, TextGate)` / `elif
/// callable(gate)` branches) — modeled as an enum here instead of two separate `Option` fields
/// on [`AgentDef`] so the two forms can't both be set at once.
#[derive(Clone)]
pub enum GateCondition {
    /// Compiled entirely server-side (inline JavaScript) — no worker round-trip needed.
    Text(TextGate),
    /// Serialized as a worker-task reference (`{"taskName": "{name}_gate"}`); see
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) for the worker that
    /// evaluates it.
    Callable(GateHandler),
}

impl std::fmt::Debug for GateCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GateCondition::Text(t) => f.debug_tuple("Text").field(t).finish(),
            GateCondition::Callable(_) => f.write_str("Callable(<predicate>)"),
        }
    }
}

impl From<TextGate> for GateCondition {
    fn from(gate: TextGate) -> Self {
        GateCondition::Text(gate)
    }
}

/// A `stop_when` predicate: given the loop context (`{"result": ..., "messages": ..., "iteration":
/// ...}`, matching python-sdk's `StopWhenEntry.__call__`'s context dict), decide whether the
/// agent loop should stop early.
///
/// `Result<bool>` rather than a bare `bool` so a caller can signal an evaluation error
/// explicitly (idiomatic for Rust) instead of panicking — [`super::runtime::AgentRuntime::serve`]
/// treats an `Err` the same way python's `StopWhenEntry.__call__` treats an exception from the
/// user callable: log it and fail *open* (`should_continue: true`, i.e. don't stop the agent
/// over a broken predicate).
///
/// `Arc`, not `Box`, for the same reason [`ToolHandler`](super::tool::ToolHandler) is: cheap to
/// clone into a worker without re-registering.
pub type StopWhenHandler =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>> + Send + Sync>;

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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    #[must_use]
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
    /// Structured-output typing for the agent's final response — see `OutputType`'s doc
    /// comment for the python-sdk mapping and this crate's narrowing.
    pub output_type: Option<OutputType>,
    /// Rule-based agent-to-agent transitions, used when `strategy = Strategy::Swarm` (python's
    /// `Agent(handoffs=[...])`).
    pub swarm_transitions: Vec<SwarmTransition>,
    /// Restricts which swarm transfer targets each agent may hand off to (map of agent name ->
    /// allowed target names), matching python's `Agent.allowed_transitions:
    /// Optional[Dict[str, List[str]]]`. Empty means unrestricted — matches python's `None`/falsy
    /// case in `HandoffCheckEntry._is_allowed`/`_register_swarm_transfer_workers`.
    pub allowed_transitions: HashMap<String, Vec<String>>,
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
    /// `PLAN_EXECUTE` planner sub-agent — produces the JSON plan the parent executes. Required
    /// when `strategy` is [`Strategy::PlanExecute`]; see [`AgentDef::with_strategy`].
    pub planner: Option<Box<AgentDef>>,
    /// `PLAN_EXECUTE` fallback sub-agent, invoked when the planner's plan fails mid-execution.
    /// Optional — `PLAN_EXECUTE` works without one.
    pub fallback: Option<Box<AgentDef>>,
    /// Turn cap applied to `fallback` once it's invoked.
    pub fallback_max_turns: Option<u32>,
    /// Reference text appended to the `PLAN_EXECUTE` planner's prompt as a
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
    /// Text this agent uses to introduce itself in group conversations (python's
    /// `Agent.introduction`, wire key `introduction`).
    pub introduction: Option<String>,
    /// Controls whether a sub-agent inherits the parent's conversation context. `"none"` gives
    /// it a fresh context (prompt only); omitted/anything else inherits the parent's (python's
    /// `Agent.include_contents`, wire key `includeContents`).
    pub include_contents: Option<String>,
    /// Tool calls to execute before the first LLM turn, results injected into context (python's
    /// `Agent.prefill_tools`, wire key `prefillTools`).
    pub prefill_tools: Vec<PrefillToolCall>,
    /// Gate condition for conditional sequential (`>>`) pipelines (python's `Agent.gate`, wire
    /// key `gate`) — see [`GateCondition`] for the two shapes this can take.
    pub gate: Option<GateCondition>,
    /// Predicate to end the agent loop early (python's `Agent.stop_when`). Serialized as a
    /// worker-task reference (wire key `stopWhen`, `{"taskName": "{name}_stop_when"}`) — see
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) for the worker that
    /// evaluates it.
    pub stop_when: Option<StopWhenHandler>,
    /// CLI command execution config, set via [`AgentDef::with_cli_commands`] (python's
    /// `Agent.cli_config`, wire key `cliConfig`). Also appends a `run_command` tool to
    /// [`AgentDef::tools`] — see `super::cli_config` for what is and isn't ported.
    pub cli_config: Option<super::cli_config::CliConfig>,
    /// Code execution config, set via [`AgentDef::with_code_execution`]/
    /// [`AgentDef::with_local_code_execution`] (python's `Agent.code_execution_config`, wire key
    /// `codeExecution`). Also appends an `execute_code` tool to [`AgentDef::tools`] — see
    /// `super::code_execution_config` for what is and isn't ported.
    pub code_execution: Option<super::code_execution_config::CodeExecutionConfig>,
    /// A "framework" marker (python's `Agent._framework`), set via
    /// [`AgentDef::with_framework`] — when `Some`, this agent serializes as a flattened
    /// passthrough of [`AgentDef::framework_config`] instead of the normal `AgentConfig` shape,
    /// matching `config_serializer.py::_serialize_agent`'s `_framework` special case. This is
    /// what lets a [`super::SkillAgent`] be nested as a sub-agent of an ordinary native agent
    /// tree (`agents=[...]`/[`ToolDef::agent`]) — see `super::skill`'s module doc for the
    /// standalone (non-nested) case, which doesn't need this field at all.
    pub framework: Option<String>,
    /// The raw wire config a `framework`-marked agent serializes verbatim (spread alongside
    /// `name`/`model`/`_framework`) — python's `Agent._framework_config`.
    pub framework_config: Option<Value>,
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
            .field("output_type", &self.output_type)
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
            .field("introduction", &self.introduction)
            .field("include_contents", &self.include_contents)
            .field("prefill_tools", &self.prefill_tools)
            .field("gate", &self.gate)
            .field("stop_when", &self.stop_when.as_ref().map(|_| "<predicate>"))
            .field("cli_config", &self.cli_config)
            .field("code_execution", &self.code_execution)
            .field("framework", &self.framework)
            .field("framework_config", &self.framework_config)
            .field("router", &self.router)
            .field(
                "swarm_transitions",
                &format!("<{} transitions>", self.swarm_transitions.len()),
            )
            .field("allowed_transitions", &self.allowed_transitions)
            .field("memory", &self.memory)
            .field("planner", &self.planner)
            .field("fallback", &self.fallback)
            .field("fallback_max_turns", &self.fallback_max_turns)
            .field("planner_context", &self.planner_context)
            .field("synthesize", &self.synthesize)
            .finish()
    }
}

impl AgentDef {
    /// Create a new agent definition. Validates `name` against `^[a-zA-Z_][a-zA-Z0-9_-]*$` up
    /// front (matches python-sdk's `Agent.__init__`) since the name doubles as the Conductor
    /// workflow name once compiled.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `name` doesn't match `^[a-zA-Z_][a-zA-Z0-9_-]*$`.
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
            output_type: None,
            swarm_transitions: Vec::new(),
            allowed_transitions: HashMap::new(),
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
            introduction: None,
            include_contents: None,
            prefill_tools: Vec::new(),
            gate: None,
            stop_when: None,
            cli_config: None,
            code_execution: None,
            framework: None,
            framework_config: None,
        })
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    #[must_use]
    pub fn with_tool(mut self, tool: ToolDef) -> Self {
        self.tools.push(tool);
        self
    }

    /// Attach CLI command execution, matching python's `Agent(cli_config=...)` /
    /// `Agent(cli_commands=True, cli_allowed_commands=[...])` flow: stores *config* and, when
    /// [`super::cli_config::CliConfig::enabled`], immediately appends the auto-built
    /// `{name}_run_command` tool to [`AgentDef::tools`] (matching python's
    /// `_attach_cli_tool`, called eagerly from `__init__` rather than deferred to a later build
    /// step).
    #[must_use]
    pub fn with_cli_commands(mut self, config: super::cli_config::CliConfig) -> Self {
        if config.enabled {
            let tool = super::cli_config::cli_command_tool(&config, Some(&self.name));
            self.tools.push(tool);
        }
        self.cli_config = Some(config);
        self
    }

    /// Attach code execution with full control, matching python's `Agent(code_execution=...)`.
    /// Stores *config* and, when
    /// [`super::code_execution_config::CodeExecutionConfig::enabled`], immediately appends the
    /// auto-built `{name}_execute_code` tool to [`AgentDef::tools`] (matching python's
    /// `_attach_code_execution_tool`, called eagerly from `__init__`).
    #[must_use]
    pub fn with_code_execution(
        mut self,
        config: super::code_execution_config::CodeExecutionConfig,
    ) -> Self {
        if config.enabled {
            let tool = super::code_execution_config::code_execution_tool(&config, Some(&self.name));
            self.tools.push(tool);
        }
        self.code_execution = Some(config);
        self
    }

    /// Shorthand for [`AgentDef::with_code_execution`], matching python's
    /// `Agent(local_code_execution=True, allowed_languages=[...], allowed_commands=[...])` flags:
    /// builds an enabled [`super::code_execution_config::CodeExecutionConfig`] with the given
    /// lists, defaulting `allowed_languages` to `["python"]` when empty (matching python's `or
    /// ["python"]` fallback) and leaving `allowed_commands` unrestricted when empty.
    #[must_use]
    pub fn with_local_code_execution(
        self,
        allowed_languages: Vec<String>,
        allowed_commands: Vec<String>,
    ) -> Self {
        let allowed_languages = if allowed_languages.is_empty() {
            vec!["python".to_owned()]
        } else {
            allowed_languages
        };
        self.with_code_execution(
            super::code_execution_config::CodeExecutionConfig::new()
                .with_allowed_languages(allowed_languages)
                .with_allowed_commands(allowed_commands),
        )
    }

    /// Mark this agent as a "framework" passthrough (python's `agent._framework = name;
    /// agent._framework_config = raw_config`) — see [`AgentDef::framework`]'s doc comment.
    /// `raw_config` is spread verbatim into the wire config alongside `name`/`model`/
    /// `_framework` when this agent is serialized (standalone or nested as a sub-agent), so it
    /// should already be in the exact wire shape the target framework normalizer expects (e.g.
    /// [`super::skill::SkillAgent::raw_config`]).
    #[must_use]
    pub fn with_framework(mut self, framework: impl Into<String>, raw_config: Value) -> Self {
        self.framework = Some(framework.into());
        self.framework_config = Some(raw_config);
        self
    }

    #[must_use]
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = ToolDef>) -> Self {
        self.tools.extend(tools);
        self
    }

    #[must_use]
    pub fn with_guardrail(mut self, guardrail: Guardrail) -> Self {
        self.guardrails.push(guardrail);
        self
    }

    #[must_use]
    pub fn with_guardrails(mut self, guardrails: impl IntoIterator<Item = Guardrail>) -> Self {
        self.guardrails.extend(guardrails);
        self
    }

    /// Add a sub-agent. Fails if its name collides with an already-added sub-agent's name
    /// (matches python's duplicate-name check in `Agent.__init__`, moved to construction time).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `agent`'s name collides with an already-added sub-agent's name.
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
    #[must_use]
    pub fn with_router(mut self, router: AgentDef) -> Self {
        self.router = Some(Box::new(router));
        self
    }

    /// Set structured-output typing for the agent's final response. Follows
    /// [`ToolDef::with_output_schema`](super::tool::ToolDef::with_output_schema)'s convention of
    /// taking a raw [`serde_json::Value`] schema from the caller rather than a `JsonSchema`
    /// generic bound — see `OutputType`'s doc comment for why.
    #[must_use]
    pub fn with_output_type(mut self, class_name: impl Into<String>, schema: Value) -> Self {
        self.output_type = Some(OutputType {
            schema,
            class_name: class_name.into(),
        });
        self
    }

    /// Add a rule-based agent-to-agent transition, used under `Strategy::Swarm` (python's
    /// `Agent(handoffs=[...])`). Accumulates like [`AgentDef::with_guardrail`] — call once per
    /// transition.
    #[must_use]
    pub fn with_swarm_transition(mut self, transition: SwarmTransition) -> Self {
        self.swarm_transitions.push(transition);
        self
    }

    /// Restrict swarm transfer targets reachable from `from_agent` to `targets`, matching
    /// python's `Agent(allowed_transitions={...})`. Accumulates — call once per source agent.
    #[must_use]
    pub fn with_allowed_transition(
        mut self,
        from_agent: impl Into<String>,
        targets: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_transitions.insert(
            from_agent.into(),
            targets.into_iter().map(Into::into).collect(),
        );
        self
    }

    /// Set the orchestration strategy.
    ///
    /// `Router` and `PlanExecute` each validate a required composition field against its
    /// *current* state at the moment this method runs: [`AgentDef::with_router`] /
    /// [`AgentDef::with_planner`] / [`AgentDef::with_tool`] / [`AgentDef::with_sub_agent`] must
    /// be called first in the builder chain, since an `Err` here consumes `self` and there's no
    /// way to retroactively satisfy the check afterwards. `Swarm` has no such requirement —
    /// matching python-sdk's `Agent.__init__` (which has no analogous check for
    /// `strategy="swarm"`), zero `swarm_transitions` is accepted too.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `strategy` requires a composition field (router/planner/tools) that hasn't been set yet -- see the per-strategy notes above.
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
        // Matches python-sdk's `Agent.__init__`: "Strategy.PLAN_EXECUTE requires tools=[...]
        // on the parent agent. These are the canonical plan-executable tools -- every op.tool
        // in the planner's JSON plan must be one of these. Listing tools here also ensures the
        // runtime starts workers for them."
        if strategy == Strategy::PlanExecute && self.tools.is_empty() {
            return Err(ConductorError::agent(
                "strategy 'plan_execute' requires tools: call `.with_tool(...)` before \
                 `.with_strategy(Strategy::PlanExecute)` (these are the canonical \
                 plan-executable tools -- every op.tool in the planner's JSON plan must be one \
                 of these, and listing them here also ensures the runtime starts workers for \
                 them)",
            ));
        }
        // Matches python-sdk's `Agent.__init__`: a PARALLEL parent needs a model for the
        // server-side aggregation step, or compilation fails with an opaque HTTP 400 ("Cannot
        // compile external agent directly") -- the server's `AgentConfig.external` is derived
        // from "no model" (`Agent.external`'s python docstring: "An agent with no model is
        // treated as external"), unconditionally, for every strategy. Auto-inherit from the
        // first child that has a model, so the common case `.with_sub_agent(a1)?
        // .with_sub_agent(a2)?.with_strategy(Strategy::Parallel)` works without repeating the
        // model on the parent. Picks the *first* match by design -- children may have differing
        // models for their own work, and the parent's model is only used for aggregation; an
        // explicit `.with_model(...)` before this call still overrides. If no child has a model
        // either, raise here rather than surfacing the opaque server 400 later.
        //
        // Also applied to SEQUENTIAL, which python's plain `Agent(agents=[...],
        // strategy=SEQUENTIAL)` constructor does *not* do -- only its `>>` operator shorthand
        // (`agent_a >> agent_b`) happens to dodge the same external-agent 400 by setting
        // `model=self.model` (the leftmost agent's model) as a side effect of chaining. Since
        // Rust has no operator-overload equivalent to `>>`, the explicit builder shape is the
        // *only* way to build a sequential pipeline here -- without this, every model-less
        // Rust sequential pipeline would be permanently uncompilable, a strictly worse gap than
        // python's (which at least has the operator escape hatch for the common case).
        if matches!(strategy, Strategy::Parallel | Strategy::Sequential)
            && self.model.is_none()
            && !self.agents.is_empty()
        {
            match self.agents.iter().find_map(|a| a.model.clone()) {
                Some(inherited) => self.model = Some(inherited),
                None => {
                    return Err(ConductorError::agent(format!(
                        "strategy '{strategy:?}' agent '{}' has no model and no child agent \
                         has one to inherit from: set a model on the parent (used for \
                         aggregation) or on at least one child",
                        self.name
                    )));
                }
            }
        }
        self.strategy = strategy;
        Ok(self)
    }

    /// Set the hard cap on conversation turns. Must be at least 1 (matches python's
    /// `max_turns < 1` check).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `max_turns` is `0`.
    pub fn with_max_turns(mut self, max_turns: u32) -> Result<Self> {
        if max_turns < 1 {
            return Err(ConductorError::agent("max_turns must be at least 1"));
        }
        self.max_turns = max_turns;
        Ok(self)
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Declared credential names, applying to every tool under this agent. These names flow
    /// end-to-end: declare here (or per-tool via [`ToolDef::with_credentials`] /
    /// [`AgentDef::with_tool_credentials`]) -> register (stamped onto `TaskDef.runtime_metadata`
    /// at registration time) -> the server resolves and delivers values back on the polled `Task`
    /// -> consume via [`Credentials::from_task`](super::Credentials::from_task) inside a tool
    /// body. See `docs/agents/secrets-and-credentials.md` for the full contract.
    #[must_use]
    pub fn with_credentials(mut self, credentials: Vec<String>) -> Self {
        self.credentials = credentials;
        self
    }

    /// Set the declared credential names on a single already-added tool by name — the
    /// non-literal-name convenience described in `docs/agents/parity-plan.md`'s tool-credential
    /// flow, equivalent to having built that tool with its own `.with_credentials(...)` up front.
    ///
    /// Errors if no tool named `tool_name` has been added yet (via [`AgentDef::with_tool`] /
    /// [`AgentDef::with_tools`]) — matching this crate's fail-fast builder convention (e.g.
    /// [`AgentDef::with_sub_agent`]'s duplicate-name check) rather than silently no-op-ing on a
    /// typo'd tool name.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if no tool named `tool_name` has been added yet via [`AgentDef::with_tool`]/[`AgentDef::with_tools`].
    pub fn with_tool_credentials(
        mut self,
        tool_name: impl AsRef<str>,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let tool_name = tool_name.as_ref();
        let tool = self
            .tools
            .iter_mut()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| {
                ConductorError::agent(format!(
                    "no tool named '{tool_name}': call with_tool(...) before \
                     with_tool_credentials(\"{tool_name}\", ...)"
                ))
            })?;
        tool.credentials = credentials;
        Ok(self)
    }

    #[must_use]
    pub fn with_required_tools(mut self, tools: Vec<String>) -> Self {
        self.required_tools = tools;
        self
    }

    #[must_use]
    pub fn with_context_window_budget(mut self, budget: u32) -> Self {
        self.context_window_budget = Some(budget);
        self
    }

    #[must_use]
    pub fn with_termination(mut self, termination: TerminationCondition) -> Self {
        self.termination = Some(termination);
        self
    }

    #[must_use]
    pub fn with_memory(mut self, memory: ConversationMemory) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Set the `PLAN_EXECUTE` planner sub-agent — the agent that produces the JSON plan the
    /// parent executes. Call this *before* `.with_strategy(Strategy::PlanExecute)`; see that
    /// method's validation.
    #[must_use]
    pub fn with_planner(mut self, planner: AgentDef) -> Self {
        self.planner = Some(Box::new(planner));
        self
    }

    /// Set the `PLAN_EXECUTE` fallback sub-agent, invoked when the planner's plan fails
    /// mid-execution. Optional — `PLAN_EXECUTE` works without one.
    #[must_use]
    pub fn with_fallback(mut self, fallback: AgentDef) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// Cap the number of turns the `fallback` agent gets once invoked.
    #[must_use]
    pub fn with_fallback_max_turns(mut self, turns: u32) -> Self {
        self.fallback_max_turns = Some(turns);
        self
    }

    /// Append one reference-text entry to the `PLAN_EXECUTE` planner's context. Matches
    /// python-sdk's bare-`str` shorthand for `Context(text=...)` — see the `planner_context`
    /// field's doc comment on [`AgentDef`] for why URL-backed entries aren't modeled here yet.
    #[must_use]
    pub fn with_planner_context(mut self, entry: impl Into<String>) -> Self {
        self.planner_context.push(entry.into());
        self
    }

    /// Bulk variant of [`with_planner_context`](Self::with_planner_context).
    #[must_use]
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
    #[must_use]
    pub fn with_synthesize(mut self, synthesize: bool) -> Self {
        self.synthesize = synthesize;
        self
    }

    /// Text this agent uses to introduce itself in group conversations.
    #[must_use]
    pub fn with_introduction(mut self, introduction: impl Into<String>) -> Self {
        self.introduction = Some(introduction.into());
        self
    }

    /// Control whether a sub-agent inherits the parent's conversation context. Pass `"none"`
    /// for a fresh context (prompt only); any other value (or leaving it unset) inherits the
    /// parent's, matching python-sdk's `include_contents`.
    #[must_use]
    pub fn with_include_contents(mut self, include_contents: impl Into<String>) -> Self {
        self.include_contents = Some(include_contents.into());
        self
    }

    /// Add a tool call to execute before the first LLM turn.
    #[must_use]
    pub fn with_prefill_tool(mut self, call: PrefillToolCall) -> Self {
        self.prefill_tools.push(call);
        self
    }

    /// Bulk variant of [`with_prefill_tool`](Self::with_prefill_tool).
    #[must_use]
    pub fn with_prefill_tools(mut self, calls: impl IntoIterator<Item = PrefillToolCall>) -> Self {
        self.prefill_tools.extend(calls);
        self
    }

    /// Set a gate condition for conditional sequential (`>>`) pipelines — either a [`TextGate`]
    /// or, via [`AgentDef::with_gate_fn`], a callable. See [`GateCondition`].
    #[must_use]
    pub fn with_gate(mut self, gate: impl Into<GateCondition>) -> Self {
        self.gate = Some(gate.into());
        self
    }

    /// Set a callable gate predicate for conditional sequential (`>>`) pipelines. `handler`
    /// receives `{"result": <this agent's output>}` and returns `true` to continue the
    /// pipeline, `false` to stop it after this agent. Registered as a `{name}_gate` worker by
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) — see [`GateHandler`].
    #[must_use]
    pub fn with_gate_fn<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<bool>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.gate = Some(GateCondition::Callable(Arc::new(move |context: Value| {
            let handler = Arc::clone(&handler);
            Box::pin(async move { handler(context).await })
        })));
        self
    }

    /// Set a predicate to end the agent loop early. `handler` receives the loop context
    /// (`{"result": ..., "messages": ..., "iteration": ...}`) and returns `true` to stop.
    /// Registered as a `{name}_stop_when` worker by
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) — see [`StopWhenHandler`].
    #[must_use]
    pub fn with_stop_when<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<bool>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.stop_when = Some(Arc::new(move |context: Value| {
            let handler = Arc::clone(&handler);
            Box::pin(async move { handler(context).await })
        }));
        self
    }

    #[must_use]
    pub fn with_metadata_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Register a [`CallbackHandler`]. Multiple registrations accumulate and are expected to
    /// chain in registration order (see `callback.rs`'s module docs) once a dispatcher exists;
    /// this crate only stores them for now — see `serializer.rs` for why they aren't serialized.
    #[must_use]
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

/// Sanitize an agent name for use in a derived task/reference name (`{name}_stop_when`,
/// `{name}_termination`, `{name}_{position}`, etc.).
///
/// `AgentDef::new`'s own name regex allows hyphens (matching python's `_VALID_NAME_RE`), but the
/// Conductor server replaces `-` with `_` wherever it derives a task/reference name from an
/// agent name — confirmed empirically: compiling a real agent named e.g. `"audit-hyphen-agent"`
/// with a termination condition returns `requiredWorkers: ["audit_hyphen_agent_termination",
/// ...]`, not the literal hyphenated name. Every worker-name and wire-reference construction
/// site in this module/`serializer.rs` that derives a name from `agent.name` must go through
/// this function, or its registered task name silently never matches what the compiled
/// workflow actually polls for.
pub(super) fn sanitize_for_task_name(name: &str) -> String {
    name.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::super::guardrail::RegexGuardrail;
    use super::*;

    fn test_tool(name: &str) -> ToolDef {
        ToolDef::function::<Value, _, _>(
            name,
            "a test tool",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        )
    }

    #[test]
    fn test_new_validates_name() {
        AgentDef::new("valid_name-1").unwrap();
        AgentDef::new("1invalid").unwrap_err();
        AgentDef::new("in valid").unwrap_err();
        AgentDef::new("").unwrap_err();
    }

    /// Regression test for a real bug found via live testing: the Conductor server replaces
    /// `-` with `_` wherever it derives a task/reference name from an agent name (confirmed by
    /// compiling a real `"audit-hyphen-agent"` and reading back `requiredWorkers`), but nothing
    /// in this crate did that sanitization before this fix -- every hyphenated agent name's
    /// `stop_when/termination/callback` worker would silently register under the wrong task name
    /// and never receive a task.
    #[test]
    fn test_sanitize_for_task_name_replaces_hyphens() {
        assert_eq!(
            sanitize_for_task_name("audit-hyphen-agent"),
            "audit_hyphen_agent"
        );
        assert_eq!(
            sanitize_for_task_name("already_underscored"),
            "already_underscored"
        );
        assert_eq!(
            sanitize_for_task_name("no-hyphens_here-either"),
            "no_hyphens_here_either"
        );
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
        agent.with_strategy(Strategy::Sequential).unwrap();
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
        agent.with_strategy(Strategy::Router).unwrap_err();
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
        agent.with_strategy(Strategy::Swarm).unwrap();
    }

    #[test]
    fn test_with_strategy_accepts_swarm_with_zero_transitions() {
        // Matches python-sdk's `Agent.__init__`, which has no check requiring
        // `handoffs` to be non-empty when `strategy="swarm"`.
        let agent = AgentDef::new("a").unwrap();
        agent.with_strategy(Strategy::Swarm).unwrap();
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
        agent
            .clone()
            .with_strategy(Strategy::PlanExecute)
            .unwrap_err();

        // Planner and a tool set first (required builder-chain order): accepted.
        let planner = AgentDef::new("planner").unwrap();
        let agent_with_planner = agent.with_planner(planner).with_tool(test_tool("t"));
        agent_with_planner
            .with_strategy(Strategy::PlanExecute)
            .unwrap();
    }

    /// Regression test for a real python-sdk check this crate was missing entirely: python's
    /// `Agent.__init__` requires `tools=[...]` on a `PLAN_EXECUTE` parent (these are the
    /// canonical plan-executable tools every `op.tool` in the planner's JSON plan must be one
    /// of), and raises before ever reaching the server. Without this, a Rust-built `PLAN_EXECUTE`
    /// agent with no tools would silently compile and fail confusingly server-side instead.
    #[test]
    fn test_with_strategy_plan_execute_requires_tools() {
        let planner = AgentDef::new("planner").unwrap();
        let agent = AgentDef::new("a").unwrap().with_planner(planner);

        // No tools yet: rejected.
        agent
            .clone()
            .with_strategy(Strategy::PlanExecute)
            .unwrap_err();

        // A tool added first: accepted.
        agent
            .with_tool(test_tool("t"))
            .with_strategy(Strategy::PlanExecute)
            .unwrap();
    }

    /// Regression test for python-sdk's PARALLEL model auto-inheritance: a PARALLEL parent with
    /// no model of its own inherits the first child's model, so
    /// `.with_sub_agent(a1)?.with_sub_agent(a2)?.with_strategy(Strategy::Parallel)` works
    /// without repeating `.with_model(...)` on the parent -- avoiding an opaque server 400
    /// ("Cannot compile external agent directly") that would otherwise surface only at compile
    /// time.
    #[test]
    fn test_with_strategy_parallel_inherits_first_child_model() {
        let child_without_model = AgentDef::new("child_a").unwrap();
        let child_with_model = AgentDef::new("child_b").unwrap().with_model("gpt-4o");
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child_without_model)
            .unwrap()
            .with_sub_agent(child_with_model)
            .unwrap()
            .with_strategy(Strategy::Parallel)
            .unwrap();

        assert_eq!(agent.model, Some("gpt-4o".to_owned()));
    }

    #[test]
    fn test_with_strategy_parallel_keeps_explicit_parent_model() {
        let child = AgentDef::new("child").unwrap().with_model("gpt-3.5");
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_model("gpt-4o")
            .with_sub_agent(child)
            .unwrap()
            .with_strategy(Strategy::Parallel)
            .unwrap();

        // Explicit parent model always wins over inheritance.
        assert_eq!(agent.model, Some("gpt-4o".to_owned()));
    }

    #[test]
    fn test_with_strategy_parallel_errors_when_no_model_anywhere() {
        let child = AgentDef::new("child").unwrap();
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child)
            .unwrap();

        agent.with_strategy(Strategy::Parallel).unwrap_err();
    }

    /// Matches the model-inheriting half of python's `>>` operator (`Agent.__rshift__`'s
    /// `model=self.model`) -- the only python construction path that avoids the same
    /// external-agent 400 for a model-less sequential pipeline. Rust has no operator-overload
    /// equivalent, so the plain builder path needs this itself; see the comment on
    /// `with_strategy`.
    #[test]
    fn test_with_strategy_sequential_inherits_first_child_model() {
        let researcher = AgentDef::new("researcher").unwrap().with_model("gpt-4o");
        let writer = AgentDef::new("writer").unwrap();
        let pipeline = AgentDef::new("content_pipeline")
            .unwrap()
            .with_sub_agent(researcher)
            .unwrap()
            .with_sub_agent(writer)
            .unwrap()
            .with_strategy(Strategy::Sequential)
            .unwrap();

        assert_eq!(pipeline.model, Some("gpt-4o".to_owned()));
    }

    #[test]
    fn test_with_strategy_sequential_errors_when_no_model_anywhere() {
        let child = AgentDef::new("child").unwrap();
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child)
            .unwrap();

        agent.with_strategy(Strategy::Sequential).unwrap_err();
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
        parent.with_sub_agent(child_b).unwrap_err();
    }

    #[test]
    fn test_max_turns_validation() {
        let agent = AgentDef::new("a").unwrap();
        agent.clone().with_max_turns(0).unwrap_err();
        agent.with_max_turns(1).unwrap();
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
        let debug_str = format!("{agent:?}");
        assert!(debug_str.contains("AgentDef"));
        assert!(debug_str.contains("1 handlers"));
    }

    #[test]
    fn test_with_credentials_sets_field() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.credentials.is_empty());

        let agent = agent.with_credentials(vec!["GH_TOKEN".to_owned()]);
        assert_eq!(agent.credentials, vec!["GH_TOKEN".to_owned()]);
    }

    #[test]
    fn test_with_tool_credentials_sets_named_tool() {
        let tool = ToolDef::function::<Value, _, _>(
            "create_issue",
            "files an issue",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        );
        let agent = AgentDef::new("filer")
            .unwrap()
            .with_tool(tool)
            .with_tool_credentials("create_issue", vec!["GH_TOKEN".to_owned()])
            .unwrap();

        assert_eq!(agent.tools[0].credentials, vec!["GH_TOKEN".to_owned()]);
    }

    #[test]
    fn test_with_tool_credentials_errors_on_unknown_tool_name() {
        let agent = AgentDef::new("a").unwrap();
        agent
            .with_tool_credentials("does_not_exist", vec!["GH_TOKEN".to_owned()])
            .unwrap_err();
    }

    #[test]
    fn test_with_output_type_sets_field() {
        let agent = AgentDef::new("a").unwrap();
        assert!(agent.output_type.is_none());

        let schema = serde_json::json!({"type": "object"});
        let agent = agent.with_output_type("MyOutput", schema.clone());
        assert_eq!(
            agent.output_type,
            Some(OutputType {
                schema,
                class_name: "MyOutput".to_owned(),
            })
        );
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

    #[test]
    fn test_with_gate_accepts_text_gate() {
        let agent = AgentDef::new("a").unwrap().with_gate(TextGate::new("DONE"));
        assert!(matches!(agent.gate, Some(GateCondition::Text(_))));
    }

    #[test]
    fn test_with_gate_fn_sets_callable_variant() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_gate_fn(|context: Value| async move { Ok(context["result"] == "DONE") });
        assert!(matches!(agent.gate, Some(GateCondition::Callable(_))));
    }

    #[tokio::test]
    async fn test_with_gate_fn_handler_is_callable() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_gate_fn(|context: Value| async move { Ok(context["result"] == "DONE") });
        let Some(GateCondition::Callable(handler)) = agent.gate else {
            panic!("expected callable gate");
        };
        assert!(handler(serde_json::json!({"result": "DONE"}))
            .await
            .unwrap());
        assert!(!handler(serde_json::json!({"result": "other"}))
            .await
            .unwrap());
    }

    #[test]
    fn test_with_gate_replaces_previous_gate() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_gate(TextGate::new("A"))
            .with_gate_fn(|_: Value| async move { Ok(true) });
        assert!(matches!(agent.gate, Some(GateCondition::Callable(_))));
    }

    #[test]
    fn test_with_framework_sets_marker_and_raw_config() {
        let agent = AgentDef::new("skill_agent")
            .unwrap()
            .with_framework("skill", serde_json::json!({"skillMd": "..."}));
        assert_eq!(agent.framework, Some("skill".to_owned()));
        assert_eq!(
            agent.framework_config,
            Some(serde_json::json!({"skillMd": "..."}))
        );
    }
}
