// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// Composable lifecycle hooks for agent execution.
//
// CallbackHandler defines six overridable hooks — on_agent_start/on_agent_end,
// on_model_start/on_model_end, on_tool_start/on_tool_end — fired around agent, model,
// and tool lifecycle events. Each hook takes a CallbackContext and returns Option<Value>:
// None defers to the next handler in a chain, Some(value) short-circuits it. All hooks
// default to None. This module defines only the hook contract; chaining/dispatch and
// registration on AgentDef are handled elsewhere.

use async_trait::async_trait;
use serde_json::{Map, Value};

/// Arbitrary JSON payload passed to a [`CallbackHandler`] hook.
///
/// Each hook position receives a different field set (e.g. `on_model_start` gets `messages`,
/// `on_model_end` gets `llm_result`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallbackContext {
    fields: Map<String, Value>,
}

impl CallbackContext {
    /// An empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style insert, matching this crate's `with_*` convention (see [`AgentDef`](super::def::AgentDef)).
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Look up a single field by name (e.g. `"messages"`, `"llm_result"`).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// The full backing field map, for handlers that want to inspect everything at once.
    #[must_use]
    pub fn as_map(&self) -> &Map<String, Value> {
        &self.fields
    }
}

impl From<Map<String, Value>> for CallbackContext {
    fn from(fields: Map<String, Value>) -> Self {
        Self { fields }
    }
}

/// Composable lifecycle hook for agent execution.
///
/// Implementors override only the hooks they care about; all six default to `None` ("do
/// nothing, defer to the next handler").
#[async_trait]
pub trait CallbackHandler: Send + Sync {
    /// Called before the agent begins processing.
    async fn on_agent_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after the agent finishes processing.
    async fn on_agent_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called before each LLM call.
    async fn on_model_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after each LLM call.
    async fn on_model_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called before each tool execution.
    async fn on_tool_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after each tool execution.
    async fn on_tool_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Handler that overrides nothing; every hook resolves via the trait defaults.
    struct NoopHandler;

    #[async_trait]
    impl CallbackHandler for NoopHandler {}

    #[tokio::test]
    async fn test_default_hooks_return_none() {
        let handler = NoopHandler;
        let ctx = CallbackContext::new();
        assert_eq!(handler.on_agent_start(&ctx).await, None);
        assert_eq!(handler.on_agent_end(&ctx).await, None);
        assert_eq!(handler.on_model_start(&ctx).await, None);
        assert_eq!(handler.on_model_end(&ctx).await, None);
        assert_eq!(handler.on_tool_start(&ctx).await, None);
        assert_eq!(handler.on_tool_end(&ctx).await, None);
    }

    // Mock handler exercising all six hooks, recording call order and returning a distinct
    // value from each.
    struct RecordingHandler {
        calls: Mutex<Vec<&'static str>>,
    }

    impl RecordingHandler {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl CallbackHandler for RecordingHandler {
        async fn on_agent_start(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_agent_start");
            ctx.get("input").cloned()
        }

        async fn on_agent_end(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_agent_end");
            Some(Value::String("agent_end".to_owned()))
        }

        async fn on_model_start(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_model_start");
            ctx.get("messages").cloned()
        }

        async fn on_model_end(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_model_end");
            ctx.get("llm_result").cloned()
        }

        async fn on_tool_start(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_tool_start");
            None
        }

        async fn on_tool_end(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push("on_tool_end");
            Some(Value::String("tool_end".to_owned()))
        }
    }

    #[tokio::test]
    async fn test_all_six_hooks_invoked_and_recorded() {
        let handler = RecordingHandler::new();
        let start_ctx = CallbackContext::new().with_field("input", Value::String("hi".into()));
        let model_start_ctx =
            CallbackContext::new().with_field("messages", serde_json::json!(["hello"]));
        let model_end_ctx =
            CallbackContext::new().with_field("llm_result", Value::String("hi back".into()));
        let empty_ctx = CallbackContext::new();

        assert_eq!(
            handler.on_agent_start(&start_ctx).await,
            Some(Value::String("hi".to_owned()))
        );
        assert_eq!(
            handler.on_agent_end(&empty_ctx).await,
            Some(Value::String("agent_end".to_owned()))
        );
        assert_eq!(
            handler.on_model_start(&model_start_ctx).await,
            Some(serde_json::json!(["hello"]))
        );
        assert_eq!(
            handler.on_model_end(&model_end_ctx).await,
            Some(Value::String("hi back".to_owned()))
        );
        assert_eq!(handler.on_tool_start(&empty_ctx).await, None);
        assert_eq!(
            handler.on_tool_end(&empty_ctx).await,
            Some(Value::String("tool_end".to_owned()))
        );

        assert_eq!(
            handler.calls(),
            vec![
                "on_agent_start",
                "on_agent_end",
                "on_model_start",
                "on_model_end",
                "on_tool_start",
                "on_tool_end",
            ]
        );
    }

    // Confirms the trait stays object-safe with #[async_trait] and dispatches through a
    // boxed trait object.
    #[tokio::test]
    async fn test_usable_as_boxed_trait_object() {
        let handlers: Vec<Box<dyn CallbackHandler>> =
            vec![Box::new(NoopHandler), Box::new(RecordingHandler::new())];
        let ctx = CallbackContext::new();
        for handler in &handlers {
            // Only asserting this compiles and runs without panicking through the trait object;
            // NoopHandler returns None, RecordingHandler returns Some(..) — both are valid.
            let _ = handler.on_agent_start(&ctx).await;
        }
        assert_eq!(handlers.len(), 2);
    }

    #[test]
    fn test_callback_context_field_access() {
        let ctx = CallbackContext::new()
            .with_field("tool_name", Value::String("lookup_weather".to_owned()))
            .with_field("arguments", serde_json::json!({"city": "Tokyo"}));

        assert_eq!(
            ctx.get("tool_name"),
            Some(&Value::String("lookup_weather".to_owned()))
        );
        assert_eq!(
            ctx.get("arguments"),
            Some(&serde_json::json!({"city": "Tokyo"}))
        );
        assert_eq!(ctx.get("missing"), None);
        assert_eq!(ctx.as_map().len(), 2);
    }

    #[test]
    fn test_callback_context_from_map() {
        let mut map = Map::new();
        map.insert("k".to_owned(), Value::from(1));
        let ctx = CallbackContext::from(map);
        assert_eq!(ctx.get("k"), Some(&Value::from(1)));
    }
}
