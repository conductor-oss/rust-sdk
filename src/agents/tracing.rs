// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Observability helpers mirroring python-sdk's OpenTelemetry-based `tracing.py`.
//!
//! ## A deliberate reinterpretation, not a literal port
//!
//! Python's module wraps the `opentelemetry-api` package directly and is a no-op unless that
//! package happens to be installed. This crate already depends on the `tracing` crate (Rust's
//! own structured-logging/spans ecosystem — a different thing from OpenTelemetry itself, though
//! commonly bridged to it) for its own internal logging (e.g. `runtime.rs`'s `tracing::error!`
//! calls). Building this module on `tracing` instead of adding a direct `opentelemetry` crate
//! dependency is the idiomatic Rust choice for the same reason python's design is idiomatic
//! python: a `tracing::info_span!` call costs effectively nothing when no subscriber is
//! registered, and becomes a *real* OpenTelemetry span automatically if the caller adds the
//! `tracing-opentelemetry` bridge crate and registers its layer — no code in this crate needs to
//! change either way, matching python's "automatically instruments... only activates if
//! opentelemetry-api is installed" behavior with a lighter, ecosystem-native mechanism instead
//! of a second observability dependency.
//!
//! ## Not wired into `AgentRuntime` — matching python exactly, not a gap
//!
//! Confirmed by reading `runtime.py` directly: none of `trace_agent_run`/`trace_compile`/
//! `trace_llm_call`/`trace_tool_call`/`trace_handoff` are called anywhere inside python's own
//! runtime, or from anywhere in python-sdk besides this module and one example. They are a
//! purely opt-in library a *caller* wraps their own `runtime.run(...)` call with (see python's
//! module doc for the exact usage pattern). This module is the same — nothing in `runtime.rs`
//! calls it.
//!
//! ## Span names and fields match python's exactly
//!
//! `agent.run`, `agent.compile`, `agent.llm_call`, `agent.tool_call`, `agent.handoff` — so a
//! caller who bridges to a real OTel backend sees the same span taxonomy regardless of which
//! SDK produced it.

use std::future::Future;

use tracing::field::Empty;
use tracing::{Instrument, Span};

use crate::error::Result;

/// `true` if a `tracing` subscriber is currently registered as the global default — i.e.
/// whether the spans this module builds will actually be recorded anywhere. Mirrors python's
/// `is_tracing_enabled()`, which checks whether `opentelemetry-api` is installed; the Rust
/// equivalent question is whether anything is listening, not whether a crate is present (there
/// is no "not installed" state for a dependency that's already compiled in).
pub fn is_tracing_enabled() -> bool {
    tracing::dispatcher::has_been_set()
}

/// Build the `agent.run` span for a top-level agent execution (python's `trace_agent_run`).
/// `session_id` is recorded only when non-empty, matching python's `if session_id:` check.
pub fn agent_run_span(agent_name: &str, prompt: &str, model: &str, session_id: &str) -> Span {
    let span = tracing::info_span!(
        "agent.run",
        agent.name = agent_name,
        agent.model = model,
        agent.prompt_length = prompt.len(),
        agent.session_id = Empty,
    );
    if !session_id.is_empty() {
        span.record("agent.session_id", session_id);
    }
    span
}

/// Run `f` inside an `agent.run` span, recording an error event on failure — the async
/// equivalent of python's `with trace_agent_run(...) as span: ... except Exception: span.
/// set_status(ERROR); span.record_exception(e); raise`. `tracing` has no direct "span status"
/// API (that's an OTel-SDK-specific concept); recording an `tracing::error!` event scoped to the
/// span is the idiomatic `tracing` equivalent, and is what the `tracing-opentelemetry` bridge
/// maps back to a failed span.
pub async fn traced_agent_run<F, Fut, T>(
    agent_name: &str,
    prompt: &str,
    model: &str,
    session_id: &str,
    f: F,
) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let span = agent_run_span(agent_name, prompt, model, session_id);
    async move {
        let result = f().await;
        if let Err(e) = &result {
            tracing::error!(error = %e, "agent.run failed");
        }
        result
    }
    .instrument(span)
    .await
}

/// Build the `agent.compile` span for workflow compilation (python's `trace_compile`).
/// `strategy` is recorded only when non-empty, matching python's `if strategy:` check.
pub fn compile_span(agent_name: &str, strategy: &str) -> Span {
    let span = tracing::info_span!(
        "agent.compile",
        agent.name = agent_name,
        agent.strategy = Empty,
    );
    if !strategy.is_empty() {
        span.record("agent.strategy", strategy);
    }
    span
}

/// Build the `agent.llm_call` span for one LLM invocation (python's `trace_llm_call`).
/// `prompt_tokens`/`completion_tokens` are typically unknown until the call completes; record
/// them on the returned [`Span`] afterward via [`Span::record`] (matching python, which sets
/// these attributes *after* yielding), or via [`record_token_usage`].
pub fn llm_call_span(agent_name: &str, model: &str) -> Span {
    tracing::info_span!(
        "agent.llm_call",
        agent.name = agent_name,
        llm.model = model,
        llm.prompt_tokens = Empty,
        llm.completion_tokens = Empty,
        llm.total_tokens = Empty,
    )
}

/// Build the `agent.tool_call` span for one tool execution (python's `trace_tool_call`).
/// `args`, if given, is recorded as its `Display`/`Debug`-style string form truncated to 1000
/// characters — matching python's `str(args)[:1000]`.
pub fn tool_call_span(agent_name: &str, tool_name: &str, args: Option<&str>) -> Span {
    let span = tracing::info_span!(
        "agent.tool_call",
        agent.name = agent_name,
        tool.name = tool_name,
        tool.args = Empty,
    );
    if let Some(args) = args {
        let truncated: String = args.chars().take(1000).collect();
        span.record("tool.args", truncated.as_str());
    }
    span
}

/// Run `f` inside an `agent.tool_call` span, recording an error event on failure — same
/// error-handling shape as [`traced_agent_run`], matching python's `trace_tool_call`'s
/// try/except/record_exception/raise.
pub async fn traced_tool_call<F, Fut, T>(
    agent_name: &str,
    tool_name: &str,
    args: Option<&str>,
    f: F,
) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let span = tool_call_span(agent_name, tool_name, args);
    async move {
        let result = f().await;
        if let Err(e) = &result {
            tracing::error!(error = %e, "agent.tool_call failed");
        }
        result
    }
    .instrument(span)
    .await
}

/// Build the `agent.handoff` span for an agent-to-agent transition (python's `trace_handoff`).
pub fn handoff_span(source_agent: &str, target_agent: &str) -> Span {
    tracing::info_span!(
        "agent.handoff",
        handoff.source = source_agent,
        handoff.target = target_agent,
    )
}

/// Record token usage on an existing span (python's `record_token_usage`). Each count is
/// recorded only when non-zero, matching python's `if prompt_tokens:` / etc. checks. The span
/// must have declared these fields (e.g. via [`llm_call_span`]) — recording an undeclared field
/// name is a silent no-op in `tracing`, the same "does nothing if not set up right" shape as
/// python's `if span is None or not _HAS_OTEL: return`.
pub fn record_token_usage(
    span: &Span,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
) {
    if prompt_tokens > 0 {
        span.record("llm.prompt_tokens", prompt_tokens);
    }
    if completion_tokens > 0 {
        span.record("llm.completion_tokens", completion_tokens);
    }
    if total_tokens > 0 {
        span.record("llm.total_tokens", total_tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ConductorError;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::subscriber::Subscriber;

    /// A minimal test subscriber that records every field value it's given, keyed by field
    /// name, across span creation and `record()` calls. Just enough to assert this module
    /// actually emits the fields/spans it claims to, without pulling in a full tracing test
    /// harness crate.
    #[derive(Default)]
    struct RecordingSubscriber {
        fields: Arc<Mutex<std::collections::HashMap<String, String>>>,
        span_names: Arc<Mutex<Vec<&'static str>>>,
    }

    struct FieldVisitor<'a>(&'a mut std::collections::HashMap<String, String>);

    impl Visit for FieldVisitor<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    impl Subscriber for RecordingSubscriber {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, span: &Attributes<'_>) -> Id {
            self.span_names.lock().unwrap().push(span.metadata().name());
            let mut fields = self.fields.lock().unwrap();
            span.record(&mut FieldVisitor(&mut fields));
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, values: &Record<'_>) {
            let mut fields = self.fields.lock().unwrap();
            values.record(&mut FieldVisitor(&mut fields));
        }

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
        fn event(&self, _event: &tracing::Event<'_>) {}
        fn enter(&self, _span: &Id) {}
        fn exit(&self, _span: &Id) {}
    }

    #[test]
    fn test_agent_run_span_records_expected_fields() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let span_names = Arc::new(Mutex::new(Vec::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: span_names.clone(),
        };

        tracing::subscriber::with_default(subscriber, || {
            let _span = agent_run_span("my_agent", "Hello!", "openai/gpt-4o", "sess-1");
        });

        assert_eq!(span_names.lock().unwrap().as_slice(), ["agent.run"]);
        let fields = fields.lock().unwrap();
        assert_eq!(fields.get("agent.name").unwrap(), "\"my_agent\"");
        assert_eq!(fields.get("agent.model").unwrap(), "\"openai/gpt-4o\"");
        assert_eq!(fields.get("agent.session_id").unwrap(), "\"sess-1\"");
    }

    #[test]
    fn test_agent_run_span_omits_session_id_when_empty() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };

        tracing::subscriber::with_default(subscriber, || {
            let _span = agent_run_span("my_agent", "Hello!", "openai/gpt-4o", "");
        });

        // `Empty` fields that are never `.record()`-ed are not visited at all.
        assert!(!fields.lock().unwrap().contains_key("agent.session_id"));
    }

    #[tokio::test]
    async fn test_traced_agent_run_returns_ok_result() {
        let result: Result<i32> = traced_agent_run("a", "p", "m", "", || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_traced_agent_run_propagates_error() {
        let result: Result<i32> = traced_agent_run("a", "p", "m", "", || async {
            Err(ConductorError::agent("boom"))
        })
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_traced_tool_call_returns_ok_result() {
        let result: Result<i32> = traced_tool_call("a", "t", None, || async { Ok(7) }).await;
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn test_compile_span_omits_strategy_when_empty() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };
        tracing::subscriber::with_default(subscriber, || {
            let _span = compile_span("a", "");
        });
        assert!(!fields.lock().unwrap().contains_key("agent.strategy"));
    }

    #[test]
    fn test_compile_span_records_strategy_when_given() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };
        tracing::subscriber::with_default(subscriber, || {
            let _span = compile_span("a", "plan_execute");
        });
        assert_eq!(
            fields.lock().unwrap().get("agent.strategy").unwrap(),
            "\"plan_execute\""
        );
    }

    #[test]
    fn test_tool_call_span_truncates_args_to_1000_chars() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };
        let long_args = "x".repeat(2000);
        tracing::subscriber::with_default(subscriber, || {
            let _span = tool_call_span("a", "t", Some(&long_args));
        });
        let recorded = fields.lock().unwrap().get("tool.args").unwrap().clone();
        // recorded is the Debug form of a &str, so it's wrapped in quotes -- strip those before
        // measuring length.
        assert_eq!(recorded.len(), 1002);
    }

    #[test]
    fn test_handoff_span_records_source_and_target() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };
        tracing::subscriber::with_default(subscriber, || {
            let _span = handoff_span("agent_a", "agent_b");
        });
        let fields = fields.lock().unwrap();
        assert_eq!(fields.get("handoff.source").unwrap(), "\"agent_a\"");
        assert_eq!(fields.get("handoff.target").unwrap(), "\"agent_b\"");
    }

    #[test]
    fn test_record_token_usage_only_records_nonzero_counts() {
        let fields = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let subscriber = RecordingSubscriber {
            fields: fields.clone(),
            span_names: Arc::new(Mutex::new(Vec::new())),
        };
        tracing::subscriber::with_default(subscriber, || {
            let span = llm_call_span("a", "m");
            record_token_usage(&span, 100, 0, 100);
        });
        let fields = fields.lock().unwrap();
        assert!(fields.contains_key("llm.prompt_tokens"));
        assert!(!fields.contains_key("llm.completion_tokens"));
        assert!(fields.contains_key("llm.total_tokens"));
    }

    #[test]
    fn test_is_tracing_enabled_reflects_subscriber_registration() {
        // Outside any `with_default` scope in this test's own thread, there's no reliable way
        // to assert `false` (another test on another thread may have set a global default),
        // so only assert the `true` case, inside a scope that's guaranteed to have one.
        let subscriber = RecordingSubscriber::default();
        tracing::subscriber::with_default(subscriber, || {
            assert!(is_tracing_enabled());
        });
    }
}
