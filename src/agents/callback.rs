// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Composable lifecycle hooks for agent execution.
//!
//! Ports python-sdk's `CallbackHandler` (`conductor.ai.agents.callback`) — a base class with six
//! overridable hook methods (`on_agent_start` / `on_agent_end`, `on_model_start` /
//! `on_model_end`, `on_tool_start` / `on_tool_end`) that fire around agent, model, and tool
//! lifecycle events. Multiple handlers are meant to chain on the same agent, running in
//! registration order with "first non-empty result wins" short-circuit semantics — see
//! python's `_chain_callbacks_for_position` for the reference algorithm. This module defines
//! only the hook contract ([`CallbackHandler`] + [`CallbackContext`]); the chaining/dispatch
//! logic and the `AgentDef` registration point (`callbacks: Vec<Box<dyn CallbackHandler>>`) are
//! deliberately left to a follow-up change — see `docs/agents/parity-plan.md`, which marks
//! `CallbackHandler` as a `<<trait>>` connected to `AgentDef` by an *open* circle (`o--`), not a
//! filled one: a handler is a trait object registered by the caller at run time, not data that
//! `AgentDef` owns and serializes into `agentConfig` the way `ToolDef` or `Guardrail` are.
//!
//! ## Why a trait, not a boxed `Fn` like [`ToolHandler`](super::tool::ToolHandler)
//!
//! `ToolDef` uses `ToolHandler`, a single `Arc<dyn Fn(Value) -> Fut>` type alias, because a tool
//! has exactly one call shape: arguments in, result out. A callback handler instead exposes six
//! independent, individually-overridable hook points that all share the same "no-op unless
//! overridden" default — python expresses that with a base class carrying six default methods
//! subclasses selectively override. A single `Fn` alias can't represent six independently
//! optional entry points behind one registration slot; a trait with defaulted methods can, and
//! that is exactly what `parity-plan.md`'s class diagram calls for (`<<trait>>`), so this module
//! follows the diagram rather than the `tool.rs` precedent.
//!
//! ## Why `async_trait`, not a plain sync trait
//!
//! Python's hook methods are plain `def` (synchronous) — invoked from a sync chaining helper.
//! This crate's [`Worker`](crate::worker::Worker) trait, the closest existing precedent for "a
//! trait a caller implements and this crate stores as a boxed trait object," is instead defined
//! with `#[async_trait]` (`async-trait` is already a workspace dependency — see `Cargo.toml`)
//! precisely because handlers registered into an async runtime may need to do async work of
//! their own (write to a metrics store, call an audit-log service, etc.) without blocking the
//! executor thread. `CallbackHandler` follows that same convention rather than python's
//! synchronous one: matching `Worker`'s established async-trait shape in this codebase takes
//! priority over matching python's sync methods verbatim, since the wire/behavioral contract
//! (six named hooks, `Option` return, chain-until-non-empty semantics) is what parity actually
//! requires — *how* a Rust caller is allowed to implement a hook body is not.
//!
//! ## Hook contract
//!
//! Every hook takes a [`CallbackContext`] — an arbitrary keyword-style JSON payload standing in
//! for python's `**kwargs: Any` (each hook position receives a different, evolving field set on
//! the server side: `on_model_start` gets `messages`, `on_model_end` gets `llm_result`, and so
//! on — see python's `CallbackEntry.__call__`) — and returns `Option<Value>`:
//! - `None` means "continue to the next handler in the chain" (python: return nothing / `None`).
//! - `Some(value)` means "short-circuit the remaining handlers in the chain and use `value` as
//!   the override" (python: return a non-empty `dict`). A future dispatcher should treat
//!   `Some(Value::Null)` the same as `None` if it ever arises, matching python's "non-empty
//!   dict" check rather than a bare truthiness check on `Option`.
//!
//! All six methods default to returning `None`, so an implementor overrides only the hooks it
//! cares about — matching python's base-class behavior exactly.

use async_trait::async_trait;
use serde_json::{Map, Value};

/// Arbitrary keyword-style payload passed to a [`CallbackHandler`] hook.
///
/// Stands in for python's `**kwargs: Any`: each hook position is passed a different set of
/// fields by the server/runtime (e.g. `on_model_start` receives `messages`, `on_model_end`
/// receives `llm_result` — see python-sdk's `CallbackEntry.__call__` in
/// `conductor/ai/agents/runtime/_worker_entries.py`), and that field set is expected to evolve
/// independently per position. Rust has no `**kwargs` equivalent, so this wraps the same
/// "arbitrary JSON object" shape python passes rather than giving each hook its own strongly
/// typed parameter struct — keeping the [`CallbackHandler`] trait's six method signatures stable
/// as server-side fields are added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallbackContext {
    fields: Map<String, Value>,
}

impl CallbackContext {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style insert, matching this crate's `with_*` convention (see [`AgentDef`](super::def::AgentDef)).
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Look up a single field by name (e.g. `"messages"`, `"llm_result"`).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// The full backing field map, for handlers that want to inspect everything at once.
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
/// See the module-level docs for why this is a trait (rather than a boxed `Fn` like
/// [`ToolHandler`](super::tool::ToolHandler)), why it's async (matching
/// [`Worker`](crate::worker::Worker)'s convention rather than python's synchronous methods), and
/// the chain-until-non-empty semantics a future dispatcher is expected to implement around it.
///
/// Implementors override only the hooks they care about; all six default to `None` ("do
/// nothing, defer to the next handler"), matching python-sdk's `CallbackHandler` base class.
#[async_trait]
pub trait CallbackHandler: Send + Sync {
    /// Called before the agent begins processing (python: `on_agent_start`).
    async fn on_agent_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after the agent finishes processing (python: `on_agent_end`).
    async fn on_agent_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called before each LLM call (python: `on_model_start`).
    async fn on_model_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after each LLM call (python: `on_model_end`).
    async fn on_model_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called before each tool execution (python: `on_tool_start`).
    async fn on_tool_start(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }

    /// Called after each tool execution (python: `on_tool_end`).
    async fn on_tool_end(&self, ctx: &CallbackContext) -> Option<Value> {
        let _ = ctx;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Default-only handler: overrides nothing, so every hook must resolve to `None` via the
    /// trait's default bodies (matching python's un-overridden base-class methods).
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

    /// Mock handler exercising every one of the six hooks, recording call order and returning a
    /// distinct short-circuit value from each so a test can assert both "was called" and
    /// "returned what it should."
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
            self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    #[async_trait]
    impl CallbackHandler for RecordingHandler {
        async fn on_agent_start(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_agent_start");
            ctx.get("input").cloned()
        }

        async fn on_agent_end(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_agent_end");
            Some(Value::String("agent_end".to_string()))
        }

        async fn on_model_start(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_model_start");
            ctx.get("messages").cloned()
        }

        async fn on_model_end(&self, ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_model_end");
            ctx.get("llm_result").cloned()
        }

        async fn on_tool_start(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_tool_start");
            None
        }

        async fn on_tool_end(&self, _ctx: &CallbackContext) -> Option<Value> {
            self.calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push("on_tool_end");
            Some(Value::String("tool_end".to_string()))
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
            Some(Value::String("hi".to_string()))
        );
        assert_eq!(
            handler.on_agent_end(&empty_ctx).await,
            Some(Value::String("agent_end".to_string()))
        );
        assert_eq!(
            handler.on_model_start(&model_start_ctx).await,
            Some(serde_json::json!(["hello"]))
        );
        assert_eq!(
            handler.on_model_end(&model_end_ctx).await,
            Some(Value::String("hi back".to_string()))
        );
        assert_eq!(handler.on_tool_start(&empty_ctx).await, None);
        assert_eq!(
            handler.on_tool_end(&empty_ctx).await,
            Some(Value::String("tool_end".to_string()))
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

    /// A boxed trait object is the intended storage shape (`Vec<Box<dyn CallbackHandler>>` on a
    /// future `AgentDef.callbacks` field) — confirm the trait stays object-safe with
    /// `#[async_trait]` and that dispatch through the trait object works.
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
            .with_field("tool_name", Value::String("lookup_weather".to_string()))
            .with_field("arguments", serde_json::json!({"city": "Tokyo"}));

        assert_eq!(
            ctx.get("tool_name"),
            Some(&Value::String("lookup_weather".to_string()))
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
        map.insert("k".to_string(), Value::from(1));
        let ctx = CallbackContext::from(map);
        assert_eq!(ctx.get("k"), Some(&Value::from(1)));
    }
}
