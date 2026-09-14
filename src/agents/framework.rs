// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;

use super::def::AgentDef;
use super::tool::ToolDef;

/// Generic adapter interface for an agent construct authored against someone else's agent SDK
/// (an `openai-agents` `Agent`, a LangGraph graph, a Claude Agent SDK session, ...) — see
/// `docs/agents/parity-plan.md`'s "Frameworks" section for the phasing across frameworks. A type
/// implementing this trait exposes just enough of its own shape (name, instructions/system
/// prompt, model, tool definitions) for this crate to build an [`AgentDef`] from it, without this
/// crate depending on the source framework's crate at all — [`super::framework_openai`] is the
/// one concrete, feature-gated adapter that currently exists (for `async-openai`'s tool shape);
/// nothing here depends on it.
///
/// `docs/agents/parity-plan.md` and `docs/agents/development-waves.md` describe this Wave 5 item
/// as "`From<T> for AgentDef`". Two things about that phrasing don't survive contact with the
/// actual type system, both worth spelling out so nobody "fixes" this back to match the docs:
///
/// 1. The conversion can't be infallible. [`AgentDef::new`] validates `name` against a regex and
///    returns `Result<AgentDef>` (see `src/agents/def.rs`), so building an `AgentDef` from a
///    `FrameworkAgent` is fallible in exactly the same way — a source agent with a name that
///    doesn't match `^[a-zA-Z_][a-zA-Z0-9_-]*$` must be rejectable, not panic-on-construct.
/// 2. The conversion can't be a blanket `std::convert::TryFrom` impl either, for a reason that
///    has nothing to do with this crate's design: `core` already provides
///    `impl<T, U: Into<T>> TryFrom<U> for T`, which — because `AgentDef: Into<AgentDef>` via the
///    reflexive `impl<T> From<T> for T` — already covers `TryFrom<AgentDef> for AgentDef`. Adding
///    `impl<T: FrameworkAgent> TryFrom<T> for AgentDef` on top is rejected by rustc (E0119,
///    conflicting implementations) because the compiler can't prove no type could ever implement
///    both `FrameworkAgent` and (transitively) `Into<AgentDef>` at once — this is a general
///    limitation of writing a marker-trait-bounded blanket `TryFrom` impl for a local
///    non-`Copy`/non-generic-parameter type, not something specific to `FrameworkAgent`.
///
/// [`FrameworkAgent::try_into_agent_def`] is the fallible conversion instead — a default trait
/// method, so every implementor gets it for free exactly like a blanket impl would provide, just
/// under a name `std::convert::TryFrom` isn't available for here.
///
/// Only name, instructions, model, and tool definitions are extracted here — the narrow slice of
/// `AgentDef` every source framework can plausibly express. A `FrameworkAgent` impl for a richer
/// source type (e.g. one with its own guardrail/termination/sub-agent concepts) should document,
/// on the impl itself, any source-side concept that has no `AgentDef` equivalent and is therefore
/// dropped during conversion — never drop a concept silently.
pub trait FrameworkAgent {
    /// The agent's name. Passed to [`AgentDef::new`], which validates it against
    /// `^[a-zA-Z_][a-zA-Z0-9_-]*$` — a source name that fails this check surfaces as an `Err`
    /// from [`FrameworkAgent::try_into_agent_def`], not a panic.
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

    /// Converts this framework-authored agent into an [`AgentDef`] — see the trait doc comment
    /// for why this is a default method rather than a `std::convert::TryFrom` impl.
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
            name: name.to_string(),
            description: "a bare tool".to_string(),
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
            name: "researcher".to_string(),
            instructions: Some("Find things out.".to_string()),
            model: Some("openai/gpt-4o".to_string()),
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
            name: "1-invalid".to_string(),
            instructions: None,
            model: None,
            tools: Vec::new(),
        };

        assert!(source.try_into_agent_def().is_err());
    }
}
