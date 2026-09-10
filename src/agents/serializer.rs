// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde_json::{Map, Value};

use super::def::AgentDef;
use super::guardrail::Guardrail;
use super::memory::{ConversationMemory, Message, ToolCall};
use super::swarm::SwarmTransition;
use super::termination::TerminationCondition;
use super::tool::{ToolDef, ToolType};

/// Serializes [`AgentDef`]/[`ToolDef`] trees into the `agentConfig` JSON wire format shared
/// with python-sdk's `conductor.ai.agents.config_serializer.AgentConfigSerializer`.
///
/// Hand-rolled rather than `#[derive(Serialize)]`: `ToolDef` holds a non-serializable handler
/// closure, `external` is derived from `model` rather than stored, `strategy`'s emission
/// depends on a different field (`agents`), and tool-level vs. agent-level `credentials` nest
/// at different wire locations under the same Rust field name. Every omitted `Option`/empty
/// collection is left out of the JSON entirely — never emitted as `null` or `[]`.
pub struct AgentConfigSerializer;

impl AgentConfigSerializer {
    pub fn serialize(agent: &AgentDef) -> Value {
        serialize_agent(agent)
    }
}

fn serialize_agent(agent: &AgentDef) -> Value {
    let mut map = Map::new();

    map.insert("name".to_string(), Value::String(agent.name.clone()));

    if let Some(model) = &agent.model {
        map.insert("model".to_string(), Value::String(model.clone()));
    }
    if let Some(base_url) = &agent.base_url {
        map.insert("baseUrl".to_string(), Value::String(base_url.clone()));
    }

    if !agent.agents.is_empty() {
        map.insert(
            "strategy".to_string(),
            Value::String(agent.strategy.as_str().to_string()),
        );
    }

    map.insert("maxTurns".to_string(), Value::from(agent.max_turns));
    map.insert(
        "timeoutSeconds".to_string(),
        Value::from(agent.timeout_seconds),
    );
    map.insert(
        "external".to_string(),
        Value::Bool(agent.model.as_deref().unwrap_or("").is_empty()),
    );

    if let Some(instructions) = &agent.instructions {
        map.insert(
            "instructions".to_string(),
            Value::String(instructions.clone()),
        );
    }

    if !agent.tools.is_empty() {
        map.insert(
            "tools".to_string(),
            Value::Array(agent.tools.iter().map(serialize_tool).collect()),
        );
    }

    if !agent.agents.is_empty() {
        map.insert(
            "agents".to_string(),
            Value::Array(agent.agents.iter().map(serialize_agent).collect()),
        );
    }

    // Router: this SDK version only models an agent-based router (see `AgentDef::router`'s
    // doc comment), so serialization is always the recursive full-agent shape — matching
    // python-sdk's `_serialize_router`'s `isinstance(router, Agent)` branch. There is no
    // callable-router `{"taskName": ...}` branch to reproduce here.
    if let Some(router) = &agent.router {
        map.insert("router".to_string(), serialize_agent(router));
    }

    if !agent.guardrails.is_empty() {
        map.insert(
            "guardrails".to_string(),
            Value::Array(agent.guardrails.iter().map(serialize_guardrail).collect()),
        );
    }

    if let Some(termination) = &agent.termination {
        map.insert(
            "termination".to_string(),
            serialize_termination(termination),
        );
    }

    if let Some(memory) = &agent.memory {
        map.insert("memory".to_string(), serialize_memory(memory));
    }

    // Wire key stays "handoffs" for cross-SDK compatibility even though the Rust type is named
    // `SwarmTransition` (see swarm.rs's module docs for why the Rust-side name diverges) —
    // matches python-sdk's `config_serializer.py::_serialize_agent`:
    // `if agent.handoffs: config["handoffs"] = [self._serialize_handoff(h, agent.name) for h in agent.handoffs]`.
    if !agent.swarm_transitions.is_empty() {
        map.insert(
            "handoffs".to_string(),
            Value::Array(
                agent
                    .swarm_transitions
                    .iter()
                    .map(|t| serialize_swarm_transition(t, &agent.name))
                    .collect(),
            ),
        );
    }

    // PLAN_EXECUTE named slots: planner (required by `AgentDef::with_strategy`) + fallback
    // (optional). Both serialize as full nested `agentConfig` dicts via the same
    // `serialize_agent` used for `agent.agents`/`ToolType::AgentTool` sub-agents — matches
    // python-sdk's `config["planner"] = self._serialize_agent(planner_agent)`.
    if let Some(planner) = &agent.planner {
        map.insert("planner".to_string(), serialize_agent(planner));
    }
    if let Some(fallback) = &agent.fallback {
        map.insert("fallback".to_string(), serialize_agent(fallback));
    }
    if let Some(turns) = agent.fallback_max_turns {
        map.insert("fallbackMaxTurns".to_string(), Value::from(turns));
    }

    // Planner context: bare strings normalise to python-sdk's `Context(text=...)` wire shape
    // (`{"text": ...}`) — see the `planner_context` field doc on `AgentDef` for why only that
    // shape is modeled here.
    if !agent.planner_context.is_empty() {
        map.insert(
            "plannerContext".to_string(),
            Value::Array(
                agent
                    .planner_context
                    .iter()
                    .map(|text| {
                        let mut entry = Map::new();
                        entry.insert("text".to_string(), Value::String(text.clone()));
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }

    // Synthesize flag — default true; only emitted when explicitly disabled, matching
    // python-sdk's `if not agent.synthesize: config["synthesize"] = False`.
    if !agent.synthesize {
        map.insert("synthesize".to_string(), Value::Bool(false));
    }

    if let Some(max_tokens) = agent.max_tokens {
        map.insert("maxTokens".to_string(), Value::from(max_tokens));
    }
    if let Some(budget) = agent.context_window_budget {
        map.insert("contextWindowBudget".to_string(), Value::from(budget));
    }
    if let Some(temperature) = agent.temperature {
        if let Some(n) = serde_json::Number::from_f64(temperature as f64) {
            map.insert("temperature".to_string(), Value::Number(n));
        }
    }
    if let Some(effort) = &agent.reasoning_effort {
        map.insert("reasoningEffort".to_string(), Value::String(effort.clone()));
    }

    if !agent.required_tools.is_empty() {
        map.insert(
            "requiredTools".to_string(),
            Value::Array(
                agent
                    .required_tools
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }

    if !agent.metadata.is_empty() {
        map.insert(
            "metadata".to_string(),
            Value::Object(agent.metadata.clone().into_iter().collect()),
        );
    }

    if !agent.credentials.is_empty() {
        map.insert(
            "credentials".to_string(),
            Value::Array(
                agent
                    .credentials
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }

    // `agent.callbacks` is deliberately NOT serialized here. Python's
    // `AgentConfigSerializer._serialize_agent` emits `config["callbacks"]` as a list of
    // `{"position": ..., "taskName": ...}` references (see `config_serializer.py`, around
    // `_chain_callbacks_for_position`) — each entry names a worker task, synthesized as
    // `f"{agent.name}_{position}"`, that python's `AgentRuntime` registers as a callable worker
    // at deploy time so the *server* can invoke the handler by that name. This crate has no
    // `AgentRuntime` (nothing registers a task under that name, so there is nothing for
    // `taskName` to reference), so emitting the same `{"position", "taskName"}` shape here would
    // produce a dangling reference the server can never resolve — worse than omitting the field.
    // `callbacks` stays a caller-side-only registration (see `AgentDef::with_callback`) until an
    // `AgentRuntime` follow-up exists to give each position a real task name to point at.

    Value::Object(map)
}

/// Serializes a [`Guardrail`] to the `GuardrailConfig` wire shape, matching python-sdk's
/// `AgentConfigSerializer._serialize_guardrail`: common fields (`name`/`position`/`onFail`/
/// `maxRetries`) plus the `guardrailType` discriminant and that type's own fields, contributed by
/// [`Guardrail::guardrail_type_fields`] (which delegates to the wrapped [`GuardrailCheck`](super::guardrail::GuardrailCheck)
/// — this crate always wraps a concrete checker, so the `"external"`/`"custom"` branches python's
/// version has for a `func`-less `Guardrail` don't apply here).
fn serialize_guardrail(guardrail: &Guardrail) -> Value {
    let mut map = Map::new();

    map.insert("name".to_string(), Value::String(guardrail.name.clone()));
    map.insert(
        "position".to_string(),
        Value::String(guardrail.position.as_str().to_string()),
    );
    map.insert(
        "onFail".to_string(),
        Value::String(guardrail.on_fail.as_str().to_string()),
    );
    map.insert("maxRetries".to_string(), Value::from(guardrail.max_retries));
    map.extend(guardrail.guardrail_type_fields());

    Value::Object(map)
}

/// Serializes a [`TerminationCondition`] to the `TerminationConfig` wire shape, matching
/// python-sdk's `AgentConfigSerializer._serialize_termination` exactly: a `"type"` discriminant
/// (from [`TerminationCondition::type_str`]) plus that variant's own fields, with `And`/`Or`
/// recursing into their `conditions` via this same function.
fn serialize_termination(condition: &TerminationCondition) -> Value {
    let mut map = Map::new();
    map.insert(
        "type".to_string(),
        Value::String(condition.type_str().to_string()),
    );

    match condition {
        TerminationCondition::TextMention {
            text,
            case_sensitive,
        } => {
            map.insert("text".to_string(), Value::String(text.clone()));
            map.insert("caseSensitive".to_string(), Value::Bool(*case_sensitive));
        }
        TerminationCondition::StopMessage { stop_message } => {
            map.insert(
                "stopMessage".to_string(),
                Value::String(stop_message.clone()),
            );
        }
        TerminationCondition::MaxMessage { max_messages } => {
            map.insert("maxMessages".to_string(), Value::from(*max_messages));
        }
        TerminationCondition::TokenUsage {
            max_total_tokens,
            max_prompt_tokens,
            max_completion_tokens,
        } => {
            if let Some(max_total_tokens) = max_total_tokens {
                map.insert("maxTotalTokens".to_string(), Value::from(*max_total_tokens));
            }
            if let Some(max_prompt_tokens) = max_prompt_tokens {
                map.insert(
                    "maxPromptTokens".to_string(),
                    Value::from(*max_prompt_tokens),
                );
            }
            if let Some(max_completion_tokens) = max_completion_tokens {
                map.insert(
                    "maxCompletionTokens".to_string(),
                    Value::from(*max_completion_tokens),
                );
            }
        }
        TerminationCondition::And { conditions } | TerminationCondition::Or { conditions } => {
            map.insert(
                "conditions".to_string(),
                Value::Array(conditions.iter().map(serialize_termination).collect()),
            );
        }
    }

    Value::Object(map)
}

/// Serializes a [`ConversationMemory`] to the `MemoryConfig` wire shape, matching python-sdk's
/// `AgentConfigSerializer._serialize_memory`: `messages`/`maxMessages` are each independently
/// omitted when empty/unset (`messages` per this file's usual empty-collection convention;
/// `maxMessages` per python's `if ... and memory.max_messages:` truthiness check, under which a
/// configured `0` is falsy and omitted the same as unset — mirroring the same quirk
/// [`ConversationMemory::trim`](super::memory::ConversationMemory) documents for trimming
/// itself). Note this function can return an empty object (`{}`) — python's `agent.memory` guard
/// (`if hasattr(agent, "memory") and agent.memory:`) looks like it treats an empty memory as
/// omitted too, but python's `ConversationMemory` is a plain dataclass with no `__bool__`/
/// `__len__`, so any non-`None` instance — empty or not — is truthy; the guard is really just a
/// `is not None` check. So the top-level `memory` key is emitted whenever `AgentDef.memory` is
/// `Some`, even if that produces `"memory": {}`.
fn serialize_memory(memory: &ConversationMemory) -> Value {
    let mut map = Map::new();

    if !memory.messages.is_empty() {
        map.insert(
            "messages".to_string(),
            Value::Array(memory.messages.iter().map(serialize_message).collect()),
        );
    }
    if let Some(max_messages) = memory.max_messages {
        if max_messages != 0 {
            map.insert("maxMessages".to_string(), Value::from(max_messages));
        }
    }

    Value::Object(map)
}

/// Serializes a [`Message`] to python-sdk's message dict shape (see
/// `python-sdk/src/conductor/ai/agents/memory.py`'s `add_*` methods, which build these dicts
/// directly — `message`/`tool_calls` stay snake_case while `toolCallId`/`taskReferenceName` are
/// already camelCase there, so this mirrors that mixed casing verbatim rather than normalizing
/// it). `tool_calls` is only ever populated for `MessageRole::ToolCall`; `tool_call_id`/
/// `task_reference_name` only for `MessageRole::Tool` — both omitted otherwise.
fn serialize_message(message: &Message) -> Value {
    let mut map = Map::new();

    map.insert(
        "role".to_string(),
        Value::String(message.role.as_str().to_string()),
    );
    map.insert(
        "message".to_string(),
        Value::String(message.message.clone()),
    );

    if !message.tool_calls.is_empty() {
        map.insert(
            "tool_calls".to_string(),
            Value::Array(message.tool_calls.iter().map(serialize_tool_call).collect()),
        );
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        map.insert(
            "toolCallId".to_string(),
            Value::String(tool_call_id.clone()),
        );
    }
    if let Some(task_reference_name) = &message.task_reference_name {
        map.insert(
            "taskReferenceName".to_string(),
            Value::String(task_reference_name.clone()),
        );
    }

    Value::Object(map)
}

/// Serializes a [`ToolCall`] (a [`Message`]'s `tool_calls` entry) to python-sdk's
/// `{"name", "taskReferenceName", "input"}` dict shape.
fn serialize_tool_call(tool_call: &ToolCall) -> Value {
    let mut map = Map::new();

    map.insert("name".to_string(), Value::String(tool_call.name.clone()));
    map.insert(
        "taskReferenceName".to_string(),
        Value::String(tool_call.task_reference_name.clone()),
    );
    map.insert("input".to_string(), tool_call.input.clone());

    Value::Object(map)
}

/// Serializes a [`SwarmTransition`] to the `HandoffConfig` wire shape, matching python-sdk's
/// `AgentConfigSerializer._serialize_handoff` exactly: `target` plus a `type` discriminant
/// (from [`SwarmTransition::as_str`], which already matches python's `"on_tool_result"` /
/// `"on_text_mention"` / `"on_condition"` strings) and that variant's own fields.
///
/// `OnCondition` carries an arbitrary Rust closure ([`super::swarm::SwarmConditionFn`]), which —
/// like python's `Callable[[Dict[str, Any]], bool]` — has no JSON representation. Python doesn't
/// serialize the callable either: it emits a `taskName` of `"{agent_name}_handoff_{target}"`,
/// deferring evaluation to a runtime task registered under that name
/// (`config_serializer.py`'s module docstring: "Callables ... are registered as workers ... and
/// sent as task-name references"). This crate has no `AgentRuntime` to register such a task
/// against yet, so this mirrors the wire shape (same `taskName` convention) without the runtime
/// registration side — that's out of scope here, tracked alongside the rest of the
/// `AgentRuntime` follow-up (see `docs/agents/parity-plan.md`).
fn serialize_swarm_transition(transition: &SwarmTransition, agent_name: &str) -> Value {
    let mut map = Map::new();

    map.insert(
        "target".to_string(),
        Value::String(transition.target().to_string()),
    );
    map.insert(
        "type".to_string(),
        Value::String(transition.as_str().to_string()),
    );

    match transition {
        SwarmTransition::OnToolResult {
            tool_name,
            result_contains,
            ..
        } => {
            map.insert("toolName".to_string(), Value::String(tool_name.clone()));
            if let Some(result_contains) = result_contains {
                map.insert(
                    "resultContains".to_string(),
                    Value::String(result_contains.clone()),
                );
            }
        }
        SwarmTransition::OnTextMention { text, .. } => {
            map.insert("text".to_string(), Value::String(text.clone()));
        }
        SwarmTransition::OnCondition { target, .. } => {
            map.insert(
                "taskName".to_string(),
                Value::String(format!("{agent_name}_handoff_{target}")),
            );
        }
    }

    Value::Object(map)
}

fn serialize_tool(tool: &ToolDef) -> Value {
    let mut map = Map::new();

    map.insert("name".to_string(), Value::String(tool.name.clone()));
    map.insert(
        "description".to_string(),
        Value::String(tool.description.clone()),
    );
    map.insert("inputSchema".to_string(), tool.input_schema.clone());
    map.insert(
        "toolType".to_string(),
        Value::String(tool.tool_type.as_str().to_string()),
    );

    if !tool.output_schema.is_null() {
        map.insert("outputSchema".to_string(), tool.output_schema.clone());
    }
    if tool.approval_required {
        map.insert("approvalRequired".to_string(), Value::Bool(true));
    }
    if tool.stateful {
        map.insert("stateful".to_string(), Value::Bool(true));
    }
    if let Some(timeout_seconds) = tool.timeout_seconds {
        map.insert("timeoutSeconds".to_string(), Value::from(timeout_seconds));
    }
    if let Some(max_calls) = tool.max_calls {
        map.insert("maxCalls".to_string(), Value::from(max_calls));
    }

    let mut config: Map<String, Value> = tool.config.clone().into_iter().collect();

    if tool.tool_type == ToolType::AgentTool {
        if let Some(sub_agent) = &tool.sub_agent {
            config.insert("agentConfig".to_string(), serialize_agent(sub_agent));
        }
    }

    if !tool.credentials.is_empty() {
        config.insert(
            "credentials".to_string(),
            Value::Array(
                tool.credentials
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }

    if !config.is_empty() {
        map.insert("config".to_string(), Value::Object(config));
    }

    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::super::def::Strategy;
    use super::super::guardrail::{LlmGuardrail, OnFail, Position, RegexGuardrail, RegexMode};
    use super::super::swarm::SwarmTransition;
    use super::super::termination::TerminationCondition;
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_none_values_omitted() {
        let agent = AgentDef::new("bare").unwrap();
        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        for key in [
            "model",
            "baseUrl",
            "strategy",
            "instructions",
            "tools",
            "agents",
            "router",
            "guardrails",
            "termination",
            "memory",
            "handoffs",
            "planner",
            "fallback",
            "fallbackMaxTurns",
            "plannerContext",
            "synthesize",
            "maxTokens",
            "contextWindowBudget",
            "temperature",
            "reasoningEffort",
            "requiredTools",
            "metadata",
            "credentials",
        ] {
            assert!(!obj.contains_key(key), "expected '{key}' to be omitted");
        }

        assert_eq!(obj.get("name"), Some(&Value::String("bare".to_string())));
        assert_eq!(obj.get("maxTurns"), Some(&Value::from(25u32)));
        assert_eq!(obj.get("timeoutSeconds"), Some(&Value::from(0u64)));
        assert_eq!(obj.get("external"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_strategy_only_emitted_with_sub_agents() {
        let bare = AgentDef::new("bare").unwrap();
        let bare_json = AgentConfigSerializer::serialize(&bare);
        assert!(!bare_json.as_object().unwrap().contains_key("strategy"));

        let child = AgentDef::new("child").unwrap();
        let parent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child)
            .unwrap()
            .with_strategy(Strategy::Sequential)
            .unwrap();
        let parent_json = AgentConfigSerializer::serialize(&parent);
        assert_eq!(
            parent_json.as_object().unwrap().get("strategy"),
            Some(&Value::String("sequential".to_string()))
        );
    }

    #[test]
    fn test_external_derived_from_model() {
        let no_model = AgentDef::new("a").unwrap();
        assert_eq!(
            AgentConfigSerializer::serialize(&no_model)
                .as_object()
                .unwrap()
                .get("external"),
            Some(&Value::Bool(true))
        );

        let with_model = AgentDef::new("a").unwrap().with_model("gpt-4");
        assert_eq!(
            AgentConfigSerializer::serialize(&with_model)
                .as_object()
                .unwrap()
                .get("external"),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn test_tool_credentials_nest_under_config_agent_credentials_stay_top_level() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_credentials(vec!["AGENT_CRED".to_string()])
            .with_tool(
                ToolDef::human("ask", "ask a human")
                    .with_credentials(vec!["TOOL_CRED".to_string()]),
            );

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        assert_eq!(
            obj.get("credentials"),
            Some(&Value::Array(vec![Value::String("AGENT_CRED".to_string())]))
        );

        let tool_json = &obj.get("tools").unwrap().as_array().unwrap()[0];
        let tool_config = tool_json
            .as_object()
            .unwrap()
            .get("config")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(
            tool_config.get("credentials"),
            Some(&Value::Array(vec![Value::String("TOOL_CRED".to_string())]))
        );
    }

    #[test]
    fn test_agent_tool_nests_agent_config_recursively() {
        let sub = AgentDef::new("sub").unwrap().with_model("gpt-4");
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_tool(ToolDef::agent(sub));

        let json = AgentConfigSerializer::serialize(&agent);
        let tool_json = &json
            .as_object()
            .unwrap()
            .get("tools")
            .unwrap()
            .as_array()
            .unwrap()[0];
        let config = tool_json
            .as_object()
            .unwrap()
            .get("config")
            .unwrap()
            .as_object()
            .unwrap();
        let nested_agent_config = config.get("agentConfig").unwrap().as_object().unwrap();
        assert_eq!(
            nested_agent_config.get("name"),
            Some(&Value::String("sub".to_string()))
        );
        assert_eq!(
            nested_agent_config.get("external"),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn test_serialize_regex_guardrail() {
        let checker = RegexGuardrail::new(["[\\w.+-]+@[\\w-]+\\.[\\w.-]+"])
            .unwrap()
            .with_mode(RegexMode::Allow)
            .with_message("must look like an email");
        let guardrail = Guardrail::new("no_pii", checker)
            .with_position(Position::Input)
            .unwrap()
            .with_on_fail(OnFail::Retry)
            .unwrap()
            .with_max_retries(5)
            .unwrap();
        let agent = AgentDef::new("a").unwrap().with_guardrail(guardrail);

        let json = AgentConfigSerializer::serialize(&agent);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(guardrails.len(), 1);
        let g = guardrails[0].as_object().unwrap();

        assert_eq!(g.get("name"), Some(&Value::String("no_pii".to_string())));
        assert_eq!(g.get("position"), Some(&Value::String("input".to_string())));
        assert_eq!(g.get("onFail"), Some(&Value::String("retry".to_string())));
        assert_eq!(g.get("maxRetries"), Some(&Value::from(5u32)));
        assert_eq!(
            g.get("guardrailType"),
            Some(&Value::String("regex".to_string()))
        );
        assert_eq!(
            g.get("patterns"),
            Some(&Value::Array(vec![Value::String(
                "[\\w.+-]+@[\\w-]+\\.[\\w.-]+".to_string()
            )]))
        );
        assert_eq!(g.get("mode"), Some(&Value::String("allow".to_string())));
        assert_eq!(
            g.get("message"),
            Some(&Value::String("must look like an email".to_string()))
        );
    }

    #[test]
    fn test_serialize_regex_guardrail_omits_message_when_unset() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        let agent = AgentDef::new("a").unwrap().with_guardrail(guardrail);

        let json = AgentConfigSerializer::serialize(&agent);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        let g = guardrails[0].as_object().unwrap();
        assert!(!g.contains_key("message"));
        assert_eq!(g.get("mode"), Some(&Value::String("block".to_string())));
    }

    #[test]
    fn test_serialize_llm_guardrail() {
        let checker = LlmGuardrail::new("anthropic/claude-sonnet-4-6", "no harmful content")
            .with_max_tokens(64);
        let guardrail = Guardrail::new("safety", checker);
        let agent = AgentDef::new("a").unwrap().with_guardrail(guardrail);

        let json = AgentConfigSerializer::serialize(&agent);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(guardrails.len(), 1);
        let g = guardrails[0].as_object().unwrap();

        assert_eq!(g.get("name"), Some(&Value::String("safety".to_string())));
        assert_eq!(
            g.get("position"),
            Some(&Value::String("output".to_string()))
        );
        assert_eq!(g.get("onFail"), Some(&Value::String("raise".to_string())));
        assert_eq!(g.get("maxRetries"), Some(&Value::from(3u32)));
        assert_eq!(
            g.get("guardrailType"),
            Some(&Value::String("llm".to_string()))
        );
        assert_eq!(
            g.get("model"),
            Some(&Value::String("anthropic/claude-sonnet-4-6".to_string()))
        );
        assert_eq!(
            g.get("policy"),
            Some(&Value::String("no harmful content".to_string()))
        );
        assert_eq!(g.get("maxTokens"), Some(&Value::from(64u32)));
    }

    #[test]
    fn test_serialize_llm_guardrail_omits_max_tokens_when_unset() {
        let checker = LlmGuardrail::new("openai/gpt-4o-mini", "policy text");
        let guardrail = Guardrail::new("g", checker);
        let agent = AgentDef::new("a").unwrap().with_guardrail(guardrail);

        let json = AgentConfigSerializer::serialize(&agent);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        let g = guardrails[0].as_object().unwrap();
        assert!(!g.contains_key("maxTokens"));
    }

    #[test]
    fn test_serialize_termination_text_mention() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_termination(TerminationCondition::text_mention("DONE"));

        let json = AgentConfigSerializer::serialize(&agent);
        let termination = json.as_object().unwrap().get("termination").unwrap();
        assert_eq!(
            termination,
            &serde_json::json!({
                "type": "text_mention",
                "text": "DONE",
                "caseSensitive": false,
            })
        );
    }

    #[test]
    fn test_serialize_termination_token_usage_all_limits() {
        let agent = AgentDef::new("a").unwrap().with_termination(
            TerminationCondition::token_usage(Some(10_000), Some(6_000), Some(4_000)).unwrap(),
        );

        let json = AgentConfigSerializer::serialize(&agent);
        let termination = json.as_object().unwrap().get("termination").unwrap();
        assert_eq!(
            termination,
            &serde_json::json!({
                "type": "token_usage",
                "maxTotalTokens": 10_000,
                "maxPromptTokens": 6_000,
                "maxCompletionTokens": 4_000,
            })
        );
    }

    #[test]
    fn test_serialize_termination_nested_and_or() {
        // (TextMention OR StopMessage) AND MaxMessage
        let inner_or = TerminationCondition::or(vec![
            TerminationCondition::text_mention("DONE"),
            TerminationCondition::stop_message_default(),
        ]);
        let nested = TerminationCondition::and(vec![
            inner_or,
            TerminationCondition::max_message(50).unwrap(),
        ]);

        let agent = AgentDef::new("a").unwrap().with_termination(nested);
        let json = AgentConfigSerializer::serialize(&agent);
        let termination = json.as_object().unwrap().get("termination").unwrap();

        assert_eq!(
            termination,
            &serde_json::json!({
                "type": "and",
                "conditions": [
                    {
                        "type": "or",
                        "conditions": [
                            {
                                "type": "text_mention",
                                "text": "DONE",
                                "caseSensitive": false,
                            },
                            {
                                "type": "stop_message",
                                "stopMessage": "TERMINATE",
                            },
                        ],
                    },
                    {
                        "type": "max_message",
                        "maxMessages": 50,
                    },
                ],
            })
        );
    }

    #[test]
    fn test_serialize_memory_with_messages_and_tool_call() {
        use super::super::memory::ConversationMemory;

        let mut memory = ConversationMemory::new().with_max_messages(50);
        memory.add_user_message("hi there");
        memory.add_assistant_message("hello!");
        memory.add_tool_call("search", serde_json::json!({"q": "rust"}), None);

        let agent = AgentDef::new("a").unwrap().with_memory(memory);
        let json = AgentConfigSerializer::serialize(&agent);
        let mem = json.as_object().unwrap().get("memory").unwrap();

        assert_eq!(
            mem,
            &serde_json::json!({
                "messages": [
                    {"role": "user", "message": "hi there"},
                    {"role": "assistant", "message": "hello!"},
                    {
                        "role": "tool_call",
                        "message": "",
                        "tool_calls": [
                            {
                                "name": "search",
                                "taskReferenceName": "search_ref",
                                "input": {"q": "rust"}
                            }
                        ]
                    }
                ],
                "maxMessages": 50
            })
        );
    }

    #[test]
    fn test_serialize_memory_tool_result_message_shape() {
        use super::super::memory::ConversationMemory;

        let mut memory = ConversationMemory::new();
        memory.add_tool_result("search", 42, None);

        let agent = AgentDef::new("a").unwrap().with_memory(memory);
        let json = AgentConfigSerializer::serialize(&agent);
        let mem = json.as_object().unwrap().get("memory").unwrap();

        assert_eq!(
            mem,
            &serde_json::json!({
                "messages": [
                    {
                        "role": "tool",
                        "message": "42",
                        "toolCallId": "search_ref",
                        "taskReferenceName": "search_ref"
                    }
                ]
            })
        );
    }

    #[test]
    fn test_serialize_empty_memory_is_not_omitted() {
        // Unlike `None`, an explicitly-set but empty `ConversationMemory` is NOT omitted: python's
        // guard (`if hasattr(agent, "memory") and agent.memory:`) looks like it treats an empty
        // memory as falsy, but `ConversationMemory` is a plain dataclass with no `__bool__`/
        // `__len__`, so any non-None instance is truthy and gets serialized — even to `{}`.
        use super::super::memory::ConversationMemory;

        let agent = AgentDef::new("a")
            .unwrap()
            .with_memory(ConversationMemory::new());
        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        assert_eq!(obj.get("memory"), Some(&serde_json::json!({})));
    }

    #[test]
    fn test_serialize_memory_omitted_when_none() {
        let agent = AgentDef::new("a").unwrap();
        let json = AgentConfigSerializer::serialize(&agent);
        assert!(!json.as_object().unwrap().contains_key("memory"));
    }

    #[test]
    fn test_serialize_memory_max_messages_zero_is_omitted() {
        // Matches python's `if ... and memory.max_messages:` truthiness check, under which a
        // configured `0` is falsy and omitted the same as unset.
        use super::super::memory::ConversationMemory;

        let memory = ConversationMemory::new().with_max_messages(0);
        let agent = AgentDef::new("a").unwrap().with_memory(memory);
        let json = AgentConfigSerializer::serialize(&agent);
        let mem = json.as_object().unwrap().get("memory").unwrap();
        assert!(!mem.as_object().unwrap().contains_key("maxMessages"));
    }

    #[test]
    fn test_router_serializes_nested_agent() {
        let router_agent = AgentDef::new("router_agent").unwrap().with_model("gpt-4");
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_router(router_agent)
            .with_strategy(Strategy::Router)
            .unwrap();

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();
        let router_json = obj.get("router").unwrap().as_object().unwrap();

        assert_eq!(
            router_json.get("name"),
            Some(&Value::String("router_agent".to_string()))
        );
        assert_eq!(
            router_json.get("model"),
            Some(&Value::String("gpt-4".to_string()))
        );
        assert_eq!(router_json.get("external"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_serialize_on_tool_result_transition() {
        let agent = AgentDef::new("triage")
            .unwrap()
            .with_swarm_transition(SwarmTransition::OnToolResult {
                target: "refund".into(),
                tool_name: "check_order".into(),
                result_contains: Some("eligible".into()),
            })
            .with_strategy(Strategy::Swarm)
            .unwrap();

        let json = AgentConfigSerializer::serialize(&agent);
        let handoffs = json
            .as_object()
            .unwrap()
            .get("handoffs")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(handoffs.len(), 1);
        let h = handoffs[0].as_object().unwrap();

        assert_eq!(h.get("target"), Some(&Value::String("refund".to_string())));
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_tool_result".to_string()))
        );
        assert_eq!(
            h.get("toolName"),
            Some(&Value::String("check_order".to_string()))
        );
        assert_eq!(
            h.get("resultContains"),
            Some(&Value::String("eligible".to_string()))
        );
    }

    #[test]
    fn test_serialize_on_tool_result_transition_omits_result_contains_when_unset() {
        let agent =
            AgentDef::new("triage")
                .unwrap()
                .with_swarm_transition(SwarmTransition::OnToolResult {
                    target: "refund".into(),
                    tool_name: "check_order".into(),
                    result_contains: None,
                });

        let json = AgentConfigSerializer::serialize(&agent);
        let handoffs = json
            .as_object()
            .unwrap()
            .get("handoffs")
            .unwrap()
            .as_array()
            .unwrap();
        let h = handoffs[0].as_object().unwrap();
        assert!(!h.contains_key("resultContains"));
    }

    #[test]
    fn test_serialize_on_text_mention_transition() {
        let agent = AgentDef::new("triage").unwrap().with_swarm_transition(
            SwarmTransition::OnTextMention {
                target: "filer".into(),
                text: "ACTIONABLE".into(),
            },
        );

        let json = AgentConfigSerializer::serialize(&agent);
        let handoffs = json
            .as_object()
            .unwrap()
            .get("handoffs")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(handoffs.len(), 1);
        let h = handoffs[0].as_object().unwrap();

        assert_eq!(h.get("target"), Some(&Value::String("filer".to_string())));
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_text_mention".to_string()))
        );
        assert_eq!(
            h.get("text"),
            Some(&Value::String("ACTIONABLE".to_string()))
        );
        assert!(!h.contains_key("toolName"));
        assert!(!h.contains_key("resultContains"));
    }

    /// `OnCondition` wraps an arbitrary Rust closure that, like python's `Callable[[Dict[str,
    /// Any]], bool]`, has no JSON representation. Python doesn't serialize the callable body
    /// either — `_serialize_handoff` emits a `taskName` of `"{agent_name}_handoff_{target}"`
    /// and defers evaluation to a runtime task registered under that name. This crate has no
    /// `AgentRuntime` yet to register such a task against, so this test only asserts the wire
    /// shape (the `taskName` convention) matches; actually registering/dispatching that task is
    /// out of scope until `AgentRuntime` exists.
    #[test]
    fn test_serialize_on_condition_transition_emits_task_name_reference() {
        let agent =
            AgentDef::new("triage")
                .unwrap()
                .with_swarm_transition(SwarmTransition::OnCondition {
                    target: "summarizer".into(),
                    condition: Arc::new(|ctx| ctx.tool_result.as_deref() == Some("done")),
                });

        let json = AgentConfigSerializer::serialize(&agent);
        let handoffs = json
            .as_object()
            .unwrap()
            .get("handoffs")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(handoffs.len(), 1);
        let h = handoffs[0].as_object().unwrap();

        assert_eq!(
            h.get("target"),
            Some(&Value::String("summarizer".to_string()))
        );
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_condition".to_string()))
        );
        assert_eq!(
            h.get("taskName"),
            Some(&Value::String("triage_handoff_summarizer".to_string()))
        );
    }

    #[test]
    fn test_handoffs_omitted_when_empty() {
        let agent = AgentDef::new("a").unwrap();
        let json = AgentConfigSerializer::serialize(&agent);
        assert!(!json.as_object().unwrap().contains_key("handoffs"));
    }

    #[test]
    fn test_handoffs_preserve_insertion_order() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_swarm_transition(SwarmTransition::OnTextMention {
                target: "b".into(),
                text: "first".into(),
            })
            .with_swarm_transition(SwarmTransition::OnTextMention {
                target: "c".into(),
                text: "second".into(),
            });

        let json = AgentConfigSerializer::serialize(&agent);
        let handoffs = json
            .as_object()
            .unwrap()
            .get("handoffs")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            handoffs[0].as_object().unwrap().get("target"),
            Some(&Value::String("b".to_string()))
        );
        assert_eq!(
            handoffs[1].as_object().unwrap().get("target"),
            Some(&Value::String("c".to_string()))
        );
    }

    #[test]
    fn test_plan_execute_serializes_planner_fallback_and_omits_synthesize_when_true() {
        let planner = AgentDef::new("planner").unwrap().with_model("gpt-4");
        let fallback = AgentDef::new("fallback_agent").unwrap().with_model("gpt-4");
        let agent = AgentDef::new("parent")
            .unwrap()
            .with_planner(planner)
            .with_fallback(fallback)
            .with_fallback_max_turns(3)
            .with_planner_contexts(vec!["rule one", "rule two"])
            .with_strategy(Strategy::PlanExecute)
            .unwrap();

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        // synthesize defaults to true and must be omitted, not emitted as `true`.
        assert!(!obj.contains_key("synthesize"));

        let planner_json = obj.get("planner").unwrap().as_object().unwrap();
        assert_eq!(
            planner_json.get("name"),
            Some(&Value::String("planner".to_string()))
        );
        assert_eq!(planner_json.get("external"), Some(&Value::Bool(false)));

        let fallback_json = obj.get("fallback").unwrap().as_object().unwrap();
        assert_eq!(
            fallback_json.get("name"),
            Some(&Value::String("fallback_agent".to_string()))
        );

        assert_eq!(obj.get("fallbackMaxTurns"), Some(&Value::from(3u32)));

        let planner_context = obj.get("plannerContext").unwrap().as_array().unwrap();
        assert_eq!(planner_context.len(), 2);
        assert_eq!(
            planner_context[0].as_object().unwrap().get("text"),
            Some(&Value::String("rule one".to_string()))
        );
        assert_eq!(
            planner_context[1].as_object().unwrap().get("text"),
            Some(&Value::String("rule two".to_string()))
        );

        // Explicitly disabling synthesize must emit `"synthesize": false`.
        let agent_no_synth = agent.with_synthesize(false);
        let json2 = AgentConfigSerializer::serialize(&agent_no_synth);
        assert_eq!(
            json2.as_object().unwrap().get("synthesize"),
            Some(&Value::Bool(false))
        );
    }
}
