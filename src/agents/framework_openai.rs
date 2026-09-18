// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Concrete [`FrameworkAgent`] adapter for the `async-openai` crate (Wave 5 Phase 1 — see
//! `docs/agents/README.md`'s "Frameworks" table). Gated behind the `openai-adapter` Cargo
//! feature so plain consumers of this crate never pull in `async-openai`.
//!
//! `async-openai` has no single type that bundles a name, instructions, a model, and tool
//! definitions together the way python's `openai-agents.Agent` does — the closest analog, its
//! Assistants-API `AssistantObject`, is itself a moving target (the Assistants API is being
//! phased out upstream in favor of the Responses API). What *is* stable across `async-openai`
//! versions is its tool-calling shape (`ChatCompletionTool` / `FunctionObject`), which mirrors
//! `OpenAI`'s own function-calling JSON schema. [`OpenAiAgent`] is therefore a small, local
//! bundling type — name / instructions / model, all in this crate's own shape, plus tools in
//! `async-openai`'s native [`ChatCompletionTool`] shape — so this adapter's tool-definition
//! mapping is real `async-openai` type mapping, without depending on a source "agent" object
//! whose fields may not be stable.

use std::collections::HashMap;

use async_openai::types::ChatCompletionTool;
use serde_json::Value;

use super::framework::FrameworkAgent;
use super::tool::{ToolDef, ToolType};

/// Minimal async-openai-shaped agent: bundles the fields this crate can extract from tool
/// definitions authored against `async-openai`'s chat-completions tool shape into one value
/// [`FrameworkAgent`] can be implemented against. See the module doc comment for why this is a
/// local bundling type rather than an impl directly on an `async-openai` "agent" type.
#[derive(Debug, Clone, Default)]
pub struct OpenAiAgent {
    pub name: String,
    pub instructions: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<ChatCompletionTool>,
}

impl OpenAiAgent {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instructions: None,
            model: None,
            tools: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_tool(mut self, tool: ChatCompletionTool) -> Self {
        self.tools.push(tool);
        self
    }
}

impl FrameworkAgent for OpenAiAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn instructions(&self) -> Option<String> {
        self.instructions.clone()
    }

    fn model(&self) -> Option<String> {
        self.model.clone()
    }

    /// Maps each `ChatCompletionTool` (`{"type": "function", "function": {...}}`) into a
    /// [`ToolDef`]. `async-openai`'s `ChatCompletionToolType` currently has only the `Function`
    /// variant, matching `OpenAI`'s own API — there is nothing else to narrow away here, unlike
    /// the Assistants-API `AssistantTools` enum's `code_interpreter` / `file_search` variants
    /// (server-run `OpenAI` built-ins with no `ToolDef` equivalent, since they're not Conductor
    /// worker/http/mcp/human/agent tools), which this adapter does not attempt to map.
    fn tools(&self) -> Vec<ToolDef> {
        self.tools
            .iter()
            .map(|tool| {
                let function = &tool.function;
                ToolDef {
                    name: function.name.clone(),
                    description: function.description.clone().unwrap_or_default(),
                    input_schema: function.parameters.clone().unwrap_or(Value::Null),
                    output_schema: Value::Null,
                    tool_type: ToolType::Worker,
                    approval_required: false,
                    stateful: false,
                    timeout_seconds: None,
                    max_calls: None,
                    config: HashMap::new(),
                    credentials: Vec::new(),
                    sub_agent: None,
                    handler: None,
                    guardrails: Vec::new(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_openai::types::{ChatCompletionToolType, FunctionObject};

    fn weather_tool() -> ChatCompletionTool {
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: "get_weather".to_owned(),
                description: Some("Get current weather for a city".to_owned()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "city": { "type": "string" } },
                    "required": ["city"],
                })),
                strict: None,
            },
        }
    }

    #[test]
    fn test_open_ai_agent_maps_into_agent_def() {
        let source = OpenAiAgent::new("weather_agent")
            .with_instructions("Answer weather questions.")
            .with_model("gpt-4o")
            .with_tool(weather_tool());

        let agent = source.try_into_agent_def().unwrap();

        assert_eq!(agent.name, "weather_agent");
        assert_eq!(
            agent.instructions.as_deref(),
            Some("Answer weather questions.")
        );
        assert_eq!(agent.model.as_deref(), Some("gpt-4o"));
        assert_eq!(agent.tools.len(), 1);
        assert_eq!(agent.tools[0].name, "get_weather");
        assert_eq!(agent.tools[0].tool_type, ToolType::Worker);
        assert_eq!(agent.tools[0].description, "Get current weather for a city");
    }
}
