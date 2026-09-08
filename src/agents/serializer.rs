// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde_json::{Map, Value};

use super::def::AgentDef;
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
    use super::*;

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
}
