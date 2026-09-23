// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde_json::{Map, Value};

use super::def::{sanitize_for_task_name, AgentDef, OutputType};
use super::guardrail::Guardrail;
use super::memory::{ConversationMemory, Message, ToolCall};
use super::swarm::SwarmTransition;
use super::termination::TerminationCondition;
use super::tool::{ToolDef, ToolType};

/// Serializes [`AgentDef`]/[`ToolDef`] trees into the `agentConfig` JSON wire format.
///
/// Hand-rolled rather than `#[derive(Serialize)]` because `ToolDef` holds a non-serializable
/// handler closure and several fields (e.g. `external`, derived from `model`) need custom
/// logic. Every omitted `Option`/empty collection is left out of the JSON entirely — never
/// emitted as `null` or `[]`.
pub struct AgentConfigSerializer;

impl AgentConfigSerializer {
    #[must_use]
    pub fn serialize(agent: &AgentDef) -> Value {
        serialize_agent(agent)
    }
}

fn serialize_agent(agent: &AgentDef) -> Value {
    // A framework-marked agent (see `AgentDef::with_framework`) always serializes as this
    // flattened passthrough instead of the normal `AgentConfig` shape, whether it's the
    // top-level agent or nested as a sub-agent. Unlike the normal `model` field below, `model`
    // is emitted as explicit `null` rather than omitted when unset.
    if let Some(framework) = &agent.framework {
        let mut map = Map::new();
        map.insert("name".to_owned(), Value::String(agent.name.clone()));
        map.insert(
            "model".to_owned(),
            agent
                .model
                .as_ref()
                .filter(|m| !m.is_empty())
                .map_or(Value::Null, |m| Value::String(m.clone())),
        );
        map.insert("_framework".to_owned(), Value::String(framework.clone()));
        if let Some(Value::Object(raw_config)) = &agent.framework_config {
            map.extend(raw_config.clone());
        }
        return Value::Object(map);
    }

    let mut map = Map::new();

    map.insert("name".to_owned(), Value::String(agent.name.clone()));

    if let Some(model) = &agent.model {
        map.insert("model".to_owned(), Value::String(model.clone()));
    }
    if let Some(base_url) = &agent.base_url {
        map.insert("baseUrl".to_owned(), Value::String(base_url.clone()));
    }

    // A PLAN_EXECUTE coordinator built with `.with_planner(...)` has no entries in `agents`,
    // only `planner`/`fallback`, so checking `agents` alone would silently omit `strategy` and
    // the server would default to HANDOFF.
    if !agent.agents.is_empty() || agent.planner.is_some() || agent.fallback.is_some() {
        map.insert(
            "strategy".to_owned(),
            Value::String(agent.strategy.as_str().to_owned()),
        );
    }

    map.insert("maxTurns".to_owned(), Value::from(agent.max_turns));
    map.insert(
        "timeoutSeconds".to_owned(),
        Value::from(agent.timeout_seconds),
    );
    map.insert(
        "external".to_owned(),
        Value::Bool(agent.model.as_deref().unwrap_or("").is_empty()),
    );

    if let Some(instructions) = &agent.instructions {
        map.insert(
            "instructions".to_owned(),
            Value::String(instructions.clone()),
        );
    }

    if !agent.tools.is_empty() {
        map.insert(
            "tools".to_owned(),
            Value::Array(agent.tools.iter().map(serialize_tool).collect()),
        );
    }

    if !agent.agents.is_empty() {
        map.insert(
            "agents".to_owned(),
            Value::Array(agent.agents.iter().map(serialize_agent).collect()),
        );
    }

    // Router: this SDK version only models an agent-based router, so serialization is always
    // the recursive full-agent shape.
    if let Some(router) = &agent.router {
        map.insert("router".to_owned(), serialize_agent(router));
    }

    if let Some(output_type) = &agent.output_type {
        map.insert("outputType".to_owned(), serialize_output_type(output_type));
    }

    if !agent.guardrails.is_empty() {
        map.insert(
            "guardrails".to_owned(),
            Value::Array(agent.guardrails.iter().map(serialize_guardrail).collect()),
        );
    }

    if let Some(termination) = &agent.termination {
        map.insert("termination".to_owned(), serialize_termination(termination));
    }

    if let Some(memory) = &agent.memory {
        map.insert("memory".to_owned(), serialize_memory(memory));
    }

    // Wire key stays "handoffs" even though the Rust type is named `SwarmTransition`.
    if !agent.swarm_transitions.is_empty() {
        map.insert(
            "handoffs".to_owned(),
            Value::Array(
                agent
                    .swarm_transitions
                    .iter()
                    .map(|t| serialize_swarm_transition(t, &agent.name))
                    .collect(),
            ),
        );
    }

    // Passed straight through as a `{name: [targets]}` map.
    if !agent.allowed_transitions.is_empty() {
        map.insert(
            "allowedTransitions".to_owned(),
            Value::Object(
                agent
                    .allowed_transitions
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Value::Array(v.iter().cloned().map(Value::String).collect()),
                        )
                    })
                    .collect(),
            ),
        );
    }

    // PLAN_EXECUTE named slots: planner (required by `AgentDef::with_strategy`) + fallback
    // (optional). Both serialize as full nested `agentConfig` dicts via the same
    // `serialize_agent` used for `agent.agents`/`ToolType::AgentTool` sub-agents.
    if let Some(planner) = &agent.planner {
        map.insert("planner".to_owned(), serialize_agent(planner));
    }
    if let Some(fallback) = &agent.fallback {
        map.insert("fallback".to_owned(), serialize_agent(fallback));
    }
    if let Some(turns) = agent.fallback_max_turns {
        map.insert("fallbackMaxTurns".to_owned(), Value::from(turns));
    }

    // Planner context: bare strings normalize to the `{"text": ...}` wire shape.
    if !agent.planner_context.is_empty() {
        map.insert(
            "plannerContext".to_owned(),
            Value::Array(
                agent
                    .planner_context
                    .iter()
                    .map(|text| {
                        let mut entry = Map::new();
                        entry.insert("text".to_owned(), Value::String(text.clone()));
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }

    // Synthesize flag defaults to true; only emitted when explicitly disabled.
    if !agent.synthesize {
        map.insert("synthesize".to_owned(), Value::Bool(false));
    }

    if let Some(introduction) = &agent.introduction {
        map.insert(
            "introduction".to_owned(),
            Value::String(introduction.clone()),
        );
    }

    if let Some(include_contents) = &agent.include_contents {
        map.insert(
            "includeContents".to_owned(),
            Value::String(include_contents.clone()),
        );
    }

    if !agent.prefill_tools.is_empty() {
        map.insert(
            "prefillTools".to_owned(),
            Value::Array(
                agent
                    .prefill_tools
                    .iter()
                    .map(|pt| {
                        let mut entry = Map::new();
                        entry.insert("toolName".to_owned(), Value::String(pt.tool_name.clone()));
                        entry.insert("arguments".to_owned(), pt.arguments.clone());
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
    }

    // `TextGate` serializes inline (compiled server-side, no worker); a callable serializes as
    // a worker-task reference, evaluated by the `{name}_gate` worker `AgentRuntime::serve`
    // registers.
    if let Some(gate) = &agent.gate {
        let gate_value = match gate {
            super::def::GateCondition::Text(text_gate) => {
                let mut gate_map = Map::new();
                gate_map.insert("type".to_owned(), Value::String("text_contains".to_owned()));
                gate_map.insert("text".to_owned(), Value::String(text_gate.text.clone()));
                gate_map.insert(
                    "caseSensitive".to_owned(),
                    Value::Bool(text_gate.case_sensitive),
                );
                Value::Object(gate_map)
            }
            super::def::GateCondition::Callable(_) => {
                let mut gate_map = Map::new();
                gate_map.insert(
                    "taskName".to_owned(),
                    Value::String(format!("{}_gate", sanitize_for_task_name(&agent.name))),
                );
                Value::Object(gate_map)
            }
        };
        map.insert("gate".to_owned(), gate_value);
    }

    // The predicate itself is registered as a worker by `AgentRuntime::serve`, not serialized
    // inline.
    if agent.stop_when.is_some() {
        let mut stop_when_map = Map::new();
        stop_when_map.insert(
            "taskName".to_owned(),
            Value::String(format!("{}_stop_when", sanitize_for_task_name(&agent.name))),
        );
        map.insert("stopWhen".to_owned(), Value::Object(stop_when_map));
    }

    if let Some(max_tokens) = agent.max_tokens {
        map.insert("maxTokens".to_owned(), Value::from(max_tokens));
    }
    if let Some(budget) = agent.context_window_budget {
        map.insert("contextWindowBudget".to_owned(), Value::from(budget));
    }
    if let Some(temperature) = agent.temperature {
        if let Some(n) = serde_json::Number::from_f64(f64::from(temperature)) {
            map.insert("temperature".to_owned(), Value::Number(n));
        }
    }
    if let Some(effort) = &agent.reasoning_effort {
        map.insert("reasoningEffort".to_owned(), Value::String(effort.clone()));
    }

    if !agent.required_tools.is_empty() {
        map.insert(
            "requiredTools".to_owned(),
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
            "metadata".to_owned(),
            Value::Object(agent.metadata.clone().into_iter().collect()),
        );
    }

    if !agent.credentials.is_empty() {
        map.insert(
            "credentials".to_owned(),
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

    // `agent.callbacks` is deliberately NOT serialized here: this crate has no `AgentRuntime` to
    // register a worker task for the server to invoke by name, so emitting a `{"position",
    // "taskName"}` reference would be a dangling reference the server can never resolve.
    // `callbacks` stays a caller-side-only registration (see `AgentDef::with_callback`).

    Value::Object(map)
}

// Serializes an OutputType to the OutputTypeConfig wire shape: a JSON `schema` plus the
// originating `className` for server-side validation.
fn serialize_output_type(output_type: &OutputType) -> Value {
    let mut map = Map::new();
    map.insert("schema".to_owned(), output_type.schema.clone());
    map.insert(
        "className".to_owned(),
        Value::String(output_type.class_name.clone()),
    );
    Value::Object(map)
}

// Serializes a Guardrail to the GuardrailConfig wire shape: common fields
// (`name`/`position`/`onFail`/`maxRetries`) plus the `guardrailType` discriminant and that
// type's own fields, contributed by Guardrail::guardrail_type_fields.
fn serialize_guardrail(guardrail: &Guardrail) -> Value {
    let mut map = Map::new();

    map.insert("name".to_owned(), Value::String(guardrail.name.clone()));
    map.insert(
        "position".to_owned(),
        Value::String(guardrail.position.as_str().to_owned()),
    );
    map.insert(
        "onFail".to_owned(),
        Value::String(guardrail.on_fail.as_str().to_owned()),
    );
    map.insert("maxRetries".to_owned(), Value::from(guardrail.max_retries));
    map.extend(guardrail.guardrail_type_fields());

    Value::Object(map)
}

// Serializes a TerminationCondition to the TerminationConfig wire shape: a `"type"`
// discriminant (from TerminationCondition::type_str) plus that variant's own fields, with
// `And`/`Or` recursing into their `conditions` via this same function.
fn serialize_termination(condition: &TerminationCondition) -> Value {
    let mut map = Map::new();
    map.insert(
        "type".to_owned(),
        Value::String(condition.type_str().to_owned()),
    );

    match condition {
        TerminationCondition::TextMention {
            text,
            case_sensitive,
        } => {
            map.insert("text".to_owned(), Value::String(text.clone()));
            map.insert("caseSensitive".to_owned(), Value::Bool(*case_sensitive));
        }
        TerminationCondition::StopMessage { stop_message } => {
            map.insert(
                "stopMessage".to_owned(),
                Value::String(stop_message.clone()),
            );
        }
        TerminationCondition::MaxMessage { max_messages } => {
            map.insert("maxMessages".to_owned(), Value::from(*max_messages));
        }
        TerminationCondition::TokenUsage {
            max_total_tokens,
            max_prompt_tokens,
            max_completion_tokens,
        } => {
            if let Some(max_total_tokens) = max_total_tokens {
                map.insert("maxTotalTokens".to_owned(), Value::from(*max_total_tokens));
            }
            if let Some(max_prompt_tokens) = max_prompt_tokens {
                map.insert(
                    "maxPromptTokens".to_owned(),
                    Value::from(*max_prompt_tokens),
                );
            }
            if let Some(max_completion_tokens) = max_completion_tokens {
                map.insert(
                    "maxCompletionTokens".to_owned(),
                    Value::from(*max_completion_tokens),
                );
            }
        }
        TerminationCondition::And { conditions } | TerminationCondition::Or { conditions } => {
            map.insert(
                "conditions".to_owned(),
                Value::Array(conditions.iter().map(serialize_termination).collect()),
            );
        }
    }

    Value::Object(map)
}

// Serializes a ConversationMemory to the MemoryConfig wire shape. `messages` is omitted
// when empty; `maxMessages` is omitted when unset or explicitly set to `0` (treated as falsy).
// This function can return an empty object (`{}`) — the top-level `memory` key is emitted
// whenever `AgentDef.memory` is `Some`, even if that produces `"memory": {}`.
fn serialize_memory(memory: &ConversationMemory) -> Value {
    let mut map = Map::new();

    if !memory.messages.is_empty() {
        map.insert(
            "messages".to_owned(),
            Value::Array(memory.messages.iter().map(serialize_message).collect()),
        );
    }
    if let Some(max_messages) = memory.max_messages {
        if max_messages != 0 {
            map.insert("maxMessages".to_owned(), Value::from(max_messages));
        }
    }

    Value::Object(map)
}

// Serializes a Message to its wire dict shape. Note the mixed casing: `message`/
// `tool_calls` stay `snake_case` while `toolCallId`/`taskReferenceName` are camelCase.
// `tool_calls` is only ever populated for `MessageRole::ToolCall`; `tool_call_id`/
// `task_reference_name` only for `MessageRole::Tool` — both omitted otherwise.
fn serialize_message(message: &Message) -> Value {
    let mut map = Map::new();

    map.insert(
        "role".to_owned(),
        Value::String(message.role.as_str().to_owned()),
    );
    map.insert("message".to_owned(), Value::String(message.message.clone()));

    if !message.tool_calls.is_empty() {
        map.insert(
            "tool_calls".to_owned(),
            Value::Array(message.tool_calls.iter().map(serialize_tool_call).collect()),
        );
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        map.insert("toolCallId".to_owned(), Value::String(tool_call_id.clone()));
    }
    if let Some(task_reference_name) = &message.task_reference_name {
        map.insert(
            "taskReferenceName".to_owned(),
            Value::String(task_reference_name.clone()),
        );
    }

    Value::Object(map)
}

// Serializes a ToolCall (a Message's `tool_calls` entry) to its
// `{"name", "taskReferenceName", "input"}` dict shape.
fn serialize_tool_call(tool_call: &ToolCall) -> Value {
    let mut map = Map::new();

    map.insert("name".to_owned(), Value::String(tool_call.name.clone()));
    map.insert(
        "taskReferenceName".to_owned(),
        Value::String(tool_call.task_reference_name.clone()),
    );
    map.insert("input".to_owned(), tool_call.input.clone());

    Value::Object(map)
}

// Serializes a SwarmTransition to the HandoffConfig wire shape: `target` plus a `type`
// discriminant (from SwarmTransition::as_str) and that variant's own fields.
//
// `OnCondition` carries an arbitrary Rust closure (super::swarm::SwarmConditionFn), which
// has no JSON representation, so it serializes as a `taskName` of
// `"{agent_name}_handoff_{target}"`, deferring evaluation to a runtime task registered under
// that name.
fn serialize_swarm_transition(transition: &SwarmTransition, agent_name: &str) -> Value {
    let mut map = Map::new();

    map.insert(
        "target".to_owned(),
        Value::String(transition.target().to_owned()),
    );
    map.insert(
        "type".to_owned(),
        Value::String(transition.as_str().to_owned()),
    );

    match transition {
        SwarmTransition::OnToolResult {
            tool_name,
            result_contains,
            ..
        } => {
            map.insert("toolName".to_owned(), Value::String(tool_name.clone()));
            if let Some(result_contains) = result_contains {
                map.insert(
                    "resultContains".to_owned(),
                    Value::String(result_contains.clone()),
                );
            }
        }
        SwarmTransition::OnTextMention { text, .. } => {
            map.insert("text".to_owned(), Value::String(text.clone()));
        }
        SwarmTransition::OnCondition { target, .. } => {
            map.insert(
                "taskName".to_owned(),
                Value::String(format!("{agent_name}_handoff_{target}")),
            );
        }
    }

    Value::Object(map)
}

fn serialize_tool(tool: &ToolDef) -> Value {
    let mut map = Map::new();

    map.insert("name".to_owned(), Value::String(tool.name.clone()));
    map.insert(
        "description".to_owned(),
        Value::String(tool.description.clone()),
    );
    map.insert("inputSchema".to_owned(), tool.input_schema.clone());
    map.insert(
        "toolType".to_owned(),
        Value::String(tool.tool_type.as_str().to_owned()),
    );

    if !tool.output_schema.is_null() {
        map.insert("outputSchema".to_owned(), tool.output_schema.clone());
    }
    if tool.approval_required {
        map.insert("approvalRequired".to_owned(), Value::Bool(true));
    }
    if tool.stateful {
        map.insert("stateful".to_owned(), Value::Bool(true));
    }
    if let Some(timeout_seconds) = tool.timeout_seconds {
        map.insert("timeoutSeconds".to_owned(), Value::from(timeout_seconds));
    }
    if let Some(max_calls) = tool.max_calls {
        map.insert("maxCalls".to_owned(), Value::from(max_calls));
    }
    if !tool.guardrails.is_empty() {
        map.insert(
            "guardrails".to_owned(),
            Value::Array(tool.guardrails.iter().map(serialize_guardrail).collect()),
        );
    }

    let mut config: Map<String, Value> = tool.config.clone().into_iter().collect();

    if tool.tool_type == ToolType::AgentTool {
        if let Some(sub_agent) = &tool.sub_agent {
            config.insert("agentConfig".to_owned(), serialize_agent(sub_agent));
        }
    }

    if !tool.credentials.is_empty() {
        config.insert(
            "credentials".to_owned(),
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
        map.insert("config".to_owned(), Value::Object(config));
    }

    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::super::def::{PrefillToolCall, Strategy, TextGate};
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
            "outputType",
            "guardrails",
            "termination",
            "memory",
            "handoffs",
            "allowedTransitions",
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
            "introduction",
            "includeContents",
            "prefillTools",
            "gate",
            "stopWhen",
            "cliConfig",
            "codeExecution",
        ] {
            assert!(!obj.contains_key(key), "expected '{key}' to be omitted");
        }

        assert_eq!(obj.get("name"), Some(&Value::String("bare".to_owned())));
        assert_eq!(obj.get("maxTurns"), Some(&Value::from(25_u32)));
        assert_eq!(obj.get("timeoutSeconds"), Some(&Value::from(0_u64)));
        assert_eq!(obj.get("external"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_serialize_introduction() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_introduction("Hi, I'm the billing agent.");
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("introduction"),
            Some(&Value::String("Hi, I'm the billing agent.".to_owned()))
        );
    }

    #[test]
    fn test_serialize_include_contents() {
        let agent = AgentDef::new("a").unwrap().with_include_contents("none");
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("includeContents"),
            Some(&Value::String("none".to_owned()))
        );
    }

    #[test]
    fn test_serialize_prefill_tools() {
        let agent = AgentDef::new("a").unwrap().with_prefill_tools(vec![
            PrefillToolCall::new("lookup_account", serde_json::json!({"id": "abc"})),
            PrefillToolCall::new("lookup_plan", serde_json::json!({})),
        ]);
        let json = AgentConfigSerializer::serialize(&agent);
        let prefill_tools = json.as_object().unwrap().get("prefillTools").unwrap();
        assert_eq!(
            prefill_tools,
            &serde_json::json!([
                {"toolName": "lookup_account", "arguments": {"id": "abc"}},
                {"toolName": "lookup_plan", "arguments": {}},
            ])
        );
    }

    #[test]
    fn test_serialize_gate() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_gate(TextGate::new("DONE").case_insensitive());
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("gate"),
            Some(&serde_json::json!({
                "type": "text_contains",
                "text": "DONE",
                "caseSensitive": false,
            }))
        );
    }

    #[test]
    fn test_serialize_gate_defaults_case_sensitive_true() {
        let agent = AgentDef::new("a").unwrap().with_gate(TextGate::new("DONE"));
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object()
                .unwrap()
                .get("gate")
                .unwrap()
                .get("caseSensitive"),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn test_serialize_callable_gate_as_task_name_reference() {
        let agent = AgentDef::new("triage_agent")
            .unwrap()
            .with_gate_fn(|_: Value| async move { Ok(true) });
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("gate"),
            Some(&serde_json::json!({"taskName": "triage_agent_gate"}))
        );
    }

    #[test]
    fn test_serialize_callable_gate_sanitizes_hyphens_in_task_name() {
        let agent = AgentDef::new("triage-agent")
            .unwrap()
            .with_gate_fn(|_: Value| async move { Ok(true) });
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("gate"),
            Some(&serde_json::json!({"taskName": "triage_agent_gate"}))
        );
    }

    #[test]
    fn test_serialize_stop_when_as_task_name_reference() {
        let agent = AgentDef::new("triage_agent")
            .unwrap()
            .with_stop_when(|_context: Value| async move { Ok(false) });
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("stopWhen"),
            Some(&serde_json::json!({"taskName": "triage_agent_stop_when"}))
        );
    }

    // Regression test: a hyphenated agent name's `stopWhen.taskName` must match the sanitized
    // name the server expects.
    #[test]
    fn test_serialize_stop_when_sanitizes_hyphens_in_task_name() {
        let agent = AgentDef::new("triage-agent")
            .unwrap()
            .with_stop_when(|_context: Value| async move { Ok(false) });
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("stopWhen"),
            Some(&serde_json::json!({"taskName": "triage_agent_stop_when"}))
        );
    }

    #[test]
    fn test_strategy_only_emitted_with_sub_agents() {
        let bare = AgentDef::new("bare").unwrap();
        let bare_json = AgentConfigSerializer::serialize(&bare);
        assert!(!bare_json.as_object().unwrap().contains_key("strategy"));

        let child = AgentDef::new("child").unwrap().with_model("gpt-4o");
        let parent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(child)
            .unwrap()
            .with_strategy(Strategy::Sequential)
            .unwrap();
        let parent_json = AgentConfigSerializer::serialize(&parent);
        assert_eq!(
            parent_json.as_object().unwrap().get("strategy"),
            Some(&Value::String("sequential".to_owned()))
        );
    }

    // Regression test: a `PLAN_EXECUTE` coordinator has no entries in `agents` (its sub-agents
    // live in `planner`/`fallback` instead), so checking `agents.is_empty()` alone omitted
    // `strategy` from the wire payload, and the server defaulted to `Strategy.HANDOFF`.
    #[test]
    fn test_strategy_emitted_for_plan_execute_with_only_planner_no_sub_agents() {
        let planner = AgentDef::new("planner").unwrap().with_model("gpt-4");
        let coordinator = AgentDef::new("coordinator")
            .unwrap()
            .with_model("gpt-4")
            .with_planner(planner)
            .with_tool(ToolDef::function::<Value, _, _>(
                "t",
                "a test tool",
                serde_json::json!({"type": "object"}),
                |_args: Value| async move { Ok(Value::Null) },
            ))
            .with_strategy(Strategy::PlanExecute)
            .unwrap();

        assert!(coordinator.agents.is_empty());

        let json = AgentConfigSerializer::serialize(&coordinator);
        assert_eq!(
            json.as_object().unwrap().get("strategy"),
            Some(&Value::String("plan_execute".to_owned()))
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
            .with_credentials(vec!["AGENT_CRED".to_owned()])
            .with_tool(
                ToolDef::human("ask", "ask a human").with_credentials(vec!["TOOL_CRED".to_owned()]),
            );

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        assert_eq!(
            obj.get("credentials"),
            Some(&Value::Array(vec![Value::String("AGENT_CRED".to_owned())]))
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
            Some(&Value::Array(vec![Value::String("TOOL_CRED".to_owned())]))
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
            Some(&Value::String("sub".to_owned()))
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

        assert_eq!(g.get("name"), Some(&Value::String("no_pii".to_owned())));
        assert_eq!(g.get("position"), Some(&Value::String("input".to_owned())));
        assert_eq!(g.get("onFail"), Some(&Value::String("retry".to_owned())));
        assert_eq!(g.get("maxRetries"), Some(&Value::from(5_u32)));
        assert_eq!(
            g.get("guardrailType"),
            Some(&Value::String("regex".to_owned()))
        );
        assert_eq!(
            g.get("patterns"),
            Some(&Value::Array(vec![Value::String(
                "[\\w.+-]+@[\\w-]+\\.[\\w.-]+".to_owned()
            )]))
        );
        assert_eq!(g.get("mode"), Some(&Value::String("allow".to_owned())));
        assert_eq!(
            g.get("message"),
            Some(&Value::String("must look like an email".to_owned()))
        );
    }

    #[test]
    fn test_tool_guardrails_omitted_when_empty() {
        let tool = ToolDef::human("ask", "ask a human");
        let json = serialize_tool(&tool);
        assert!(!json.as_object().unwrap().contains_key("guardrails"));
    }

    #[test]
    fn test_serialize_tool_level_guardrail() {
        let tool = ToolDef::human("ask", "ask a human").with_guardrail(Guardrail::new(
            "no_pii",
            RegexGuardrail::new(["x"]).unwrap(),
        ));
        let json = serialize_tool(&tool);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(guardrails.len(), 1);
        assert_eq!(
            guardrails[0].as_object().unwrap().get("name"),
            Some(&Value::String("no_pii".to_owned()))
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
        assert_eq!(g.get("mode"), Some(&Value::String("block".to_owned())));
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

        assert_eq!(g.get("name"), Some(&Value::String("safety".to_owned())));
        assert_eq!(g.get("position"), Some(&Value::String("output".to_owned())));
        assert_eq!(g.get("onFail"), Some(&Value::String("raise".to_owned())));
        assert_eq!(g.get("maxRetries"), Some(&Value::from(3_u32)));
        assert_eq!(
            g.get("guardrailType"),
            Some(&Value::String("llm".to_owned()))
        );
        assert_eq!(
            g.get("model"),
            Some(&Value::String("anthropic/claude-sonnet-4-6".to_owned()))
        );
        assert_eq!(
            g.get("policy"),
            Some(&Value::String("no harmful content".to_owned()))
        );
        assert_eq!(g.get("maxTokens"), Some(&Value::from(64_u32)));
    }

    #[test]
    fn test_serialize_function_guardrail() {
        let agent =
            AgentDef::new("a")
                .unwrap()
                .with_guardrail(super::super::guardrail::Guardrail::new(
                    "no_pii",
                    super::super::guardrail::FunctionGuardrail::new(|_: &str| {
                        super::super::guardrail::GuardrailResult::pass()
                    }),
                ));
        let json = AgentConfigSerializer::serialize(&agent);
        let guardrails = json
            .as_object()
            .unwrap()
            .get("guardrails")
            .unwrap()
            .as_array()
            .unwrap();
        let g = guardrails[0].as_object().unwrap();
        assert_eq!(
            g.get("guardrailType"),
            Some(&Value::String("custom".to_owned()))
        );
        assert_eq!(g.get("taskName"), Some(&Value::String("no_pii".to_owned())));
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
        // Unlike `None`, an explicitly-set but empty `ConversationMemory` is NOT omitted -- it
        // serializes even to `{}`.
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
        // A configured `0` is falsy and omitted the same as unset.
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
            Some(&Value::String("router_agent".to_owned()))
        );
        assert_eq!(
            router_json.get("model"),
            Some(&Value::String("gpt-4".to_owned()))
        );
        assert_eq!(router_json.get("external"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_serialize_output_type() {
        let schema =
            serde_json::json!({"type": "object", "properties": {"answer": {"type": "string"}}});
        let agent = AgentDef::new("a")
            .unwrap()
            .with_output_type("Answer", schema);

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        assert_eq!(
            obj.get("outputType"),
            Some(&serde_json::json!({
                "schema": {"type": "object", "properties": {"answer": {"type": "string"}}},
                "className": "Answer",
            }))
        );
    }

    #[test]
    fn test_output_type_omitted_when_none() {
        let agent = AgentDef::new("a").unwrap();
        let json = AgentConfigSerializer::serialize(&agent);
        assert!(!json.as_object().unwrap().contains_key("outputType"));
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

        assert_eq!(h.get("target"), Some(&Value::String("refund".to_owned())));
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_tool_result".to_owned()))
        );
        assert_eq!(
            h.get("toolName"),
            Some(&Value::String("check_order".to_owned()))
        );
        assert_eq!(
            h.get("resultContains"),
            Some(&Value::String("eligible".to_owned()))
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

        assert_eq!(h.get("target"), Some(&Value::String("filer".to_owned())));
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_text_mention".to_owned()))
        );
        assert_eq!(h.get("text"), Some(&Value::String("ACTIONABLE".to_owned())));
        assert!(!h.contains_key("toolName"));
        assert!(!h.contains_key("resultContains"));
    }

    // `OnCondition` wraps an arbitrary Rust closure, which has no JSON representation, so it
    // serializes as a `taskName` of `"{agent_name}_handoff_{target}"` and defers evaluation to
    // a runtime task registered under that name.
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
            Some(&Value::String("summarizer".to_owned()))
        );
        assert_eq!(
            h.get("type"),
            Some(&Value::String("on_condition".to_owned()))
        );
        assert_eq!(
            h.get("taskName"),
            Some(&Value::String("triage_handoff_summarizer".to_owned()))
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
            Some(&Value::String("b".to_owned()))
        );
        assert_eq!(
            handoffs[1].as_object().unwrap().get("target"),
            Some(&Value::String("c".to_owned()))
        );
    }

    #[test]
    fn test_serialize_framework_marked_agent_flattens_raw_config() {
        let agent = AgentDef::new("skill_agent").unwrap().with_framework(
            "skill",
            serde_json::json!({"skillMd": "...", "agentFiles": {}}),
        );
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json,
            serde_json::json!({
                "name": "skill_agent",
                "model": null,
                "_framework": "skill",
                "skillMd": "...",
                "agentFiles": {},
            })
        );
    }

    #[test]
    fn test_serialize_framework_marked_agent_includes_model_when_set() {
        let agent = AgentDef::new("skill_agent")
            .unwrap()
            .with_framework("skill", serde_json::json!({}))
            .with_model("openai/gpt-4o");
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(json["model"], serde_json::json!("openai/gpt-4o"));
    }

    #[test]
    fn test_serialize_framework_marked_agent_nested_as_sub_agent() {
        let skill_agent = AgentDef::new("skill_agent")
            .unwrap()
            .with_framework("skill", serde_json::json!({"skillMd": "..."}));
        let parent = AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(skill_agent)
            .unwrap();
        let json = AgentConfigSerializer::serialize(&parent);
        let agents = json
            .as_object()
            .unwrap()
            .get("agents")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            agents[0],
            serde_json::json!({
                "name": "skill_agent",
                "model": null,
                "_framework": "skill",
                "skillMd": "...",
            })
        );
    }

    #[test]
    fn test_allowed_transitions_omitted_when_empty() {
        let agent = AgentDef::new("a").unwrap();
        let json = AgentConfigSerializer::serialize(&agent);
        assert!(!json.as_object().unwrap().contains_key("allowedTransitions"));
    }

    #[test]
    fn test_serialize_allowed_transitions() {
        let agent = AgentDef::new("a")
            .unwrap()
            .with_allowed_transition("a", ["b", "c"])
            .with_allowed_transition("b", ["a"]);
        let json = AgentConfigSerializer::serialize(&agent);
        assert_eq!(
            json.as_object().unwrap().get("allowedTransitions"),
            Some(&serde_json::json!({"a": ["b", "c"], "b": ["a"]}))
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
            .with_tool(ToolDef::function::<Value, _, _>(
                "t",
                "a test tool",
                serde_json::json!({"type": "object"}),
                |_args: Value| async move { Ok(Value::Null) },
            ))
            .with_strategy(Strategy::PlanExecute)
            .unwrap();

        let json = AgentConfigSerializer::serialize(&agent);
        let obj = json.as_object().unwrap();

        // synthesize defaults to true and must be omitted, not emitted as `true`.
        assert!(!obj.contains_key("synthesize"));

        let planner_json = obj.get("planner").unwrap().as_object().unwrap();
        assert_eq!(
            planner_json.get("name"),
            Some(&Value::String("planner".to_owned()))
        );
        assert_eq!(planner_json.get("external"), Some(&Value::Bool(false)));

        let fallback_json = obj.get("fallback").unwrap().as_object().unwrap();
        assert_eq!(
            fallback_json.get("name"),
            Some(&Value::String("fallback_agent".to_owned()))
        );

        assert_eq!(obj.get("fallbackMaxTurns"), Some(&Value::from(3_u32)));

        let planner_context = obj.get("plannerContext").unwrap().as_array().unwrap();
        assert_eq!(planner_context.len(), 2);
        assert_eq!(
            planner_context[0].as_object().unwrap().get("text"),
            Some(&Value::String("rule one".to_owned()))
        );
        assert_eq!(
            planner_context[1].as_object().unwrap().get("text"),
            Some(&Value::String("rule two".to_owned()))
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
