// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;

use super::def::AgentDef;
use super::tool::ToolDef;

/// Adapter interface for an agent construct authored against another agent SDK. A type
/// implementing this trait exposes its name, instructions, model, and tool definitions so this
/// crate can build an [`AgentDef`] from it via [`FrameworkAgent::try_into_agent_def`], without
/// depending on the source framework's crate.
///
/// Only name, instructions, model, and tool definitions are extracted — the narrow slice of
/// `AgentDef` every source framework can plausibly express. An impl for a richer source type
/// should document any source-side concept with no `AgentDef` equivalent that gets dropped
/// during conversion — never drop a concept silently.
pub trait FrameworkAgent {
    /// The agent's name. Passed to [`AgentDef::new`], which validates it against
    /// `^[a-zA-Z_][a-zA-Z0-9_-]*$` — a name that fails this check surfaces as an `Err` from
    /// [`FrameworkAgent::try_into_agent_def`], not a panic.
    fn name(&self) -> String;

    /// The agent's instructions / system prompt, if any.
    fn instructions(&self) -> Option<String> {
        None
    }

    /// The model identifier ("provider/model", matching [`AgentDef::with_model`]'s convention),
    /// if the source framework pins one.
    fn model(&self) -> Option<String> {
        None
    }

    /// Tool definitions this agent exposes, already narrowed to this crate's [`ToolDef`] /
    /// [`super::ToolType`] shape.
    fn tools(&self) -> Vec<ToolDef> {
        Vec::new()
    }

    /// Converts this framework-authored agent into an [`AgentDef`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if [`AgentDef::new`] rejects `self.name()` (empty or invalid characters).
    fn try_into_agent_def(self) -> Result<AgentDef>
    where
        Self: Sized,
    {
        let mut agent = AgentDef::new(self.name())?;
        if let Some(instructions) = self.instructions() {
            agent = agent.with_instructions(instructions);
        }
        if let Some(model) = self.model() {
            agent = agent.with_model(model);
        }
        for tool in self.tools() {
            agent = agent.with_tool(tool);
        }
        Ok(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::ToolType;
    use serde_json::Value;
    use std::collections::HashMap;

    struct MinimalFrameworkAgent {
        name: String,
        instructions: Option<String>,
        model: Option<String>,
        tools: Vec<ToolDef>,
    }

    impl FrameworkAgent for MinimalFrameworkAgent {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn instructions(&self) -> Option<String> {
            self.instructions.clone()
        }

        fn model(&self) -> Option<String> {
            self.model.clone()
        }

        fn tools(&self) -> Vec<ToolDef> {
            self.tools.clone()
        }
    }

    fn bare_tool(name: &str) -> ToolDef {
        ToolDef {
            name: name.to_owned(),
            description: "a bare tool".to_owned(),
            input_schema: Value::Null,
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
    }

    #[test]
    fn test_try_into_agent_def_maps_expected_fields() {
        let source = MinimalFrameworkAgent {
            name: "researcher".to_owned(),
            instructions: Some("Find things out.".to_owned()),
            model: Some("openai/gpt-4o".to_owned()),
            tools: vec![bare_tool("search")],
        };

        let agent = source.try_into_agent_def().unwrap();

        assert_eq!(agent.name, "researcher");
        assert_eq!(agent.instructions.as_deref(), Some("Find things out."));
        assert_eq!(agent.model.as_deref(), Some("openai/gpt-4o"));
        assert_eq!(agent.tools.len(), 1);
        assert_eq!(agent.tools[0].name, "search");
    }

    #[test]
    fn test_try_into_agent_def_rejects_invalid_name() {
        let source = MinimalFrameworkAgent {
            name: "1-invalid".to_owned(),
            instructions: None,
            model: None,
            tools: Vec::new(),
        };

        source.try_into_agent_def().unwrap_err();
    }
}
