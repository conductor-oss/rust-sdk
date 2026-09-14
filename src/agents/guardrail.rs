// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Guardrails — input and output validation for agent responses.
//!
//! Ports python-sdk's `conductor.ai.agents.guardrail` (`guardrail.py`). Guardrails compile to
//! Conductor worker tasks positioned before ([`Position::Input`]) or after ([`Position::Output`])
//! the `LlmChatComplete` task; on failure with [`OnFail::Retry`] the guardrail's message is
//! appended to the conversation and the LLM is called again — see `rust-sdk/docs/agents/parity-plan.md`
//! (search "Guardrail") for where this sits in the overall `AgentDef` shape.
//!
//! ## Why this isn't a class hierarchy
//!
//! Python models `RegexGuardrail`/`LLMGuardrail` as subclasses of `Guardrail`: each subclass's
//! `__init__` builds a bound-method `func` (`self._check` / `self._evaluate`) and hands it to
//! `Guardrail.__init__`, which stores it and later calls `self.func(content)` from `check()`.
//! Rust has no inheritance, so this port follows `parity-plan.md`'s class diagram literally
//! instead of reproducing the hierarchy: [`Guardrail`] holds the position/on_fail/max_retries
//! config plus **one** boxed [`GuardrailCheck`] (composition — `Guardrail "1" *-- "1"
//! GuardrailCheck`), and [`RegexGuardrail`]/[`LlmGuardrail`] are concrete types that *implement*
//! [`GuardrailCheck`] (`..|>` in the diagram) rather than subclass anything. This is the same
//! "policy object plugged into a shared wrapper" shape python gets from `func`, minus the base
//! class:
//!
//! ```
//! use conductor::agents::{Guardrail, OnFail, RegexGuardrail};
//!
//! let checker = RegexGuardrail::new(["[\\w.+-]+@[\\w-]+\\.[\\w.-]+"])
//!     .unwrap()
//!     .with_message("Response must not contain email addresses.");
//! let no_pii = Guardrail::new("no_pii", checker)
//!     .with_on_fail(OnFail::Retry)
//!     .unwrap();
//! assert!(!no_pii.check("email me at a@b.com").passed);
//! ```
//!
//! ## Scope
//!
//! Python's `Guardrail` also accepts `func: None` paired with a `name` to reference an
//! **external** guardrail — a worker running elsewhere with no local check to call. The
//! `parity-plan.md` class diagram has no such "nameless/external" node (`Guardrail` always
//! composes exactly one `GuardrailCheck`), so this port always requires a concrete checker;
//! external-worker references remain out of scope (this file still does not touch `AgentDef`
//! itself — see [`FunctionGuardrail`] below for what *is* now wired into `serializer.rs`/
//! `runtime.rs`).
//!
//! Python's `@guardrail` decorator (turning a bare function into a named custom check) *is*
//! ported, as [`FunctionGuardrail`] — a third [`GuardrailCheck`] implementation alongside
//! [`RegexGuardrail`]/[`LlmGuardrail`], wrapping an arbitrary `Fn(&str) -> GuardrailResult`
//! closure instead of a decorated function (Rust has no decorator equivalent; a plain
//! higher-order constructor is the natural substitute). This is what
//! [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) registers a worker for under the
//! guardrail's own name — matching python's `_register_guardrail_worker`/
//! `_register_single_guardrail_worker`, which key off exactly this "not Regex, not LLM, not
//! external" case.

use crate::error::{ConductorError, Result};
use regex::Regex;
use serde_json::{Map, Value};
use std::sync::Arc;

// ── Enums ────────────────────────────────────────────────────────────────

/// Where a guardrail runs relative to the LLM call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Runs before the LLM call, against the incoming prompt/message. Client-side — cannot
    /// pause a workflow (see [`OnFail::Human`]).
    Input,
    /// Runs after the LLM call, against the model's response.
    Output,
}

impl Position {
    /// Wire-format string, matching python-sdk's `Position(str, Enum)` values exactly.
    pub fn as_str(&self) -> &'static str {
        match self {
            Position::Input => "input",
            Position::Output => "output",
        }
    }
}

/// What to do when a guardrail check fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnFail {
    /// Append the guardrail's message to the conversation and call the LLM again (bounded by
    /// [`Guardrail::max_retries`]).
    Retry,
    /// Raise/fail the run immediately.
    Raise,
    /// Use [`GuardrailResult::fixed_output`] in place of the original content.
    Fix,
    /// Pause the workflow for a human decision. Only valid for [`Position::Output`] — an input
    /// guardrail runs client-side and has no workflow execution to pause.
    Human,
}

impl OnFail {
    /// Wire-format string, matching python-sdk's `OnFail(str, Enum)` values exactly.
    pub fn as_str(&self) -> &'static str {
        match self {
            OnFail::Retry => "retry",
            OnFail::Raise => "raise",
            OnFail::Fix => "fix",
            OnFail::Human => "human",
        }
    }
}

/// `on_fail = Human` is only valid for `position = Output` (matches python's `ValueError` in
/// `Guardrail.__init__`: input guardrails are client-side and cannot pause a workflow).
fn validate_position_on_fail(position: Position, on_fail: OnFail) -> Result<()> {
    if on_fail == OnFail::Human && position == Position::Input {
        return Err(ConductorError::agent(
            "on_fail = Human is only valid for position = Output (input guardrails are \
             client-side and cannot pause a workflow)",
        ));
    }
    Ok(())
}

// ── GuardrailResult ─────────────────────────────────────────────────────

/// The result of a guardrail check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailResult {
    /// `true` if the content passes the guardrail.
    pub passed: bool,
    /// Feedback message — sent back to the LLM on [`OnFail::Retry`].
    pub message: String,
    /// For [`OnFail::Fix`] — the corrected output to use instead of the original. Ignored when
    /// `passed` is `true`.
    pub fixed_output: Option<String>,
}

impl GuardrailResult {
    /// A passing result with no message.
    pub fn pass() -> Self {
        Self {
            passed: true,
            message: String::new(),
            fixed_output: None,
        }
    }

    /// A failing result carrying a feedback message.
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            fixed_output: None,
        }
    }

    /// A failing result that also carries a corrected output, for [`OnFail::Fix`].
    pub fn fail_with_fix(message: impl Into<String>, fixed_output: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            fixed_output: Some(fixed_output.into()),
        }
    }
}

// ── GuardrailCheck ──────────────────────────────────────────────────────

/// A content-validation check pluggable into a [`Guardrail`].
///
/// Parallel to python's `func: Callable[[str], GuardrailResult]` slot on `Guardrail.__init__`.
/// [`RegexGuardrail`] and [`LlmGuardrail`] are this port's two concrete implementations (matching
/// `parity-plan.md`'s class diagram: `RegexGuardrail ..|> GuardrailCheck`, `LlmGuardrail ..|>
/// GuardrailCheck`). `Send + Sync` supertraits match this crate's other boxed-trait-object
/// conventions (e.g. [`CallbackHandler`](super::callback::CallbackHandler),
/// [`ToolHandler`](super::tool::ToolHandler)) so a `Guardrail` can be held and called from a
/// multi-threaded async runtime once one exists.
pub trait GuardrailCheck: Send + Sync {
    /// Run the check against `content`.
    fn check(&self, content: &str) -> GuardrailResult;

    /// Type-specific wire fields for [`AgentConfigSerializer`](super::AgentConfigSerializer) —
    /// the `guardrailType` discriminant (`"regex"`, `"llm"`, `"custom"`, ...) plus whatever
    /// fields that concrete guardrail type contributes, matching python-sdk's
    /// `AgentConfigSerializer._serialize_guardrail`'s `isinstance` branches. Each
    /// [`GuardrailCheck`] impl owns its own wire shape here rather than the serializer
    /// downcasting a `dyn GuardrailCheck` (matching how [`ToolType::as_str`](super::tool::ToolType::as_str)
    /// gives each tool-type variant its own wire representation) — see
    /// [`Guardrail::guardrail_type_fields`], the method `AgentConfigSerializer` actually calls.
    /// `name` is the wrapping [`Guardrail`]'s name, threaded through because
    /// [`FunctionGuardrail`]'s `"custom"` wire shape needs it for `taskName` (python's
    /// `result["taskName"] = guardrail.name`) — the checker itself has no name of its own.
    fn guardrail_type_fields(&self, name: &str) -> Map<String, Value>;
}

// ── Guardrail ────────────────────────────────────────────────────────────

/// A validation guardrail for agent input or output.
///
/// Wraps a [`GuardrailCheck`] — [`RegexGuardrail`], [`LlmGuardrail`], or any other implementation
/// — with the config python's `Guardrail.__init__` validates: where it runs
/// ([`Guardrail::with_position`]), what happens on failure ([`Guardrail::with_on_fail`]), and how
/// many retries it gets ([`Guardrail::with_max_retries`]).
///
/// Construct with [`Guardrail::new`] and compose with consuming `with_*` builders — matching
/// [`AgentDef`](super::def::AgentDef)'s pattern (`fn with_x(mut self, ...) -> Self`/`Result<Self>`,
/// no `&mut self` builders). Defaults match python's `Guardrail.__init__`: `position = Output`,
/// `on_fail = Raise`, `max_retries = 3`.
#[derive(Clone)]
pub struct Guardrail {
    pub name: String,
    pub position: Position,
    pub on_fail: OnFail,
    pub max_retries: u32,
    checker: Arc<dyn GuardrailCheck>,
}

impl std::fmt::Debug for Guardrail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guardrail")
            .field("name", &self.name)
            .field("position", &self.position)
            .field("on_fail", &self.on_fail)
            .field("max_retries", &self.max_retries)
            .field("checker", &"dyn GuardrailCheck")
            .finish()
    }
}

impl Guardrail {
    /// Create a new guardrail named `name`, wrapping `checker`.
    pub fn new(name: impl Into<String>, checker: impl GuardrailCheck + 'static) -> Self {
        Self {
            name: name.into(),
            position: Position::Output,
            on_fail: OnFail::Raise,
            max_retries: 3,
            checker: Arc::new(checker),
        }
    }

    /// Set where the guardrail runs. Rejects the combination this would leave in an invalid
    /// state — `position = Input` together with an already-set `on_fail = Human` — the same
    /// invariant [`Guardrail::with_on_fail`] enforces from the other direction, so the check
    /// holds regardless of which builder call comes first.
    pub fn with_position(mut self, position: Position) -> Result<Self> {
        validate_position_on_fail(position, self.on_fail)?;
        self.position = position;
        Ok(self)
    }

    /// Set what to do when the check fails. Rejects `on_fail = Human` combined with
    /// `position = Input` (matches python's `ValueError`: input guardrails are client-side and
    /// cannot pause a workflow).
    pub fn with_on_fail(mut self, on_fail: OnFail) -> Result<Self> {
        validate_position_on_fail(self.position, on_fail)?;
        self.on_fail = on_fail;
        Ok(self)
    }

    /// Set the max retry attempts used when `on_fail = Retry`. Must be at least 1 (matches
    /// python's `ValueError(f"max_retries must be >= 1, got {max_retries}")`).
    pub fn with_max_retries(mut self, max_retries: u32) -> Result<Self> {
        if max_retries < 1 {
            return Err(ConductorError::agent(format!(
                "max_retries must be >= 1, got {max_retries}"
            )));
        }
        self.max_retries = max_retries;
        Ok(self)
    }

    /// Run the wrapped [`GuardrailCheck`] against `content`.
    pub fn check(&self, content: &str) -> GuardrailResult {
        self.checker.check(content)
    }

    /// Type-specific wire fields (`guardrailType` plus that type's own fields) for
    /// [`AgentConfigSerializer`](super::AgentConfigSerializer) — delegates to the wrapped
    /// [`GuardrailCheck`] so the serializer never needs to downcast `checker`. See
    /// [`GuardrailCheck::guardrail_type_fields`].
    pub fn guardrail_type_fields(&self) -> Map<String, Value> {
        self.checker.guardrail_type_fields(&self.name)
    }
}

// ── RegexGuardrail ───────────────────────────────────────────────────────

/// Match mode for [`RegexGuardrail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexMode {
    /// Fail if any pattern matches (default) — blocklist.
    Block,
    /// Fail if NO pattern matches — allowlist.
    Allow,
}

/// A [`GuardrailCheck`] that validates content against one or more regex patterns.
///
/// By default **rejects** content that matches any of the given patterns
/// ([`RegexMode::Block`]). Use [`RegexGuardrail::with_mode`] with [`RegexMode::Allow`] to instead
/// reject content that matches none of the patterns.
///
/// Ports python-sdk's `RegexGuardrail` (`guardrail.py`) minus its `on_fail`/`position`/`name`/
/// `max_retries` constructor arguments — those live on the wrapping [`Guardrail`] here (see the
/// module docs for why: no base-class `__init__` to fold them all into).
///
/// # Example
///
/// ```
/// use conductor::agents::{GuardrailCheck, RegexGuardrail, RegexMode};
///
/// // Only allow JSON output.
/// let json_only = RegexGuardrail::new([r"^\s*[\{\[]"])
///     .unwrap()
///     .with_mode(RegexMode::Allow)
///     .with_message("Response must be valid JSON.");
/// assert!(json_only.check("{\"ok\": true}").passed);
/// assert!(!json_only.check("not json").passed);
/// ```
#[derive(Debug, Clone)]
pub struct RegexGuardrail {
    patterns: Vec<Regex>,
    pattern_strings: Vec<String>,
    mode: RegexMode,
    message: Option<String>,
}

impl RegexGuardrail {
    /// Compile one or more regex patterns. Fails if any pattern is not valid regex, or if the
    /// pattern list is empty.
    pub fn new(patterns: impl IntoIterator<Item = impl Into<String>>) -> Result<Self> {
        let pattern_strings: Vec<String> = patterns.into_iter().map(Into::into).collect();
        if pattern_strings.is_empty() {
            return Err(ConductorError::agent(
                "RegexGuardrail requires at least one pattern",
            ));
        }
        let patterns = pattern_strings
            .iter()
            .map(|p| {
                Regex::new(p)
                    .map_err(|e| ConductorError::agent(format!("invalid regex pattern '{p}': {e}")))
            })
            .collect::<Result<Vec<Regex>>>()?;
        Ok(Self {
            patterns,
            pattern_strings,
            mode: RegexMode::Block,
            message: None,
        })
    }

    /// Set the match mode (default [`RegexMode::Block`]).
    pub fn with_mode(mut self, mode: RegexMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set a custom failure message (default: an auto-generated one).
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// The original pattern strings this guardrail was constructed with.
    pub fn patterns(&self) -> &[String] {
        &self.pattern_strings
    }

    /// The configured match mode.
    pub fn mode(&self) -> RegexMode {
        self.mode
    }
}

impl GuardrailCheck for RegexGuardrail {
    fn check(&self, content: &str) -> GuardrailResult {
        let matched = self.patterns.iter().any(|p| p.is_match(content));

        match (self.mode, matched) {
            (RegexMode::Block, true) => {
                let msg = self
                    .message
                    .clone()
                    .unwrap_or_else(|| "Content matched a blocked pattern.".to_string());
                GuardrailResult::fail(msg)
            }
            (RegexMode::Allow, false) => {
                let msg = self
                    .message
                    .clone()
                    .unwrap_or_else(|| "Content did not match any allowed pattern.".to_string());
                GuardrailResult::fail(msg)
            }
            _ => GuardrailResult::pass(),
        }
    }

    fn guardrail_type_fields(&self, _name: &str) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert(
            "guardrailType".to_string(),
            Value::String("regex".to_string()),
        );
        fields.insert(
            "patterns".to_string(),
            Value::Array(
                self.pattern_strings
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        fields.insert(
            "mode".to_string(),
            Value::String(
                match self.mode {
                    RegexMode::Block => "block",
                    RegexMode::Allow => "allow",
                }
                .to_string(),
            ),
        );
        if let Some(message) = &self.message {
            fields.insert("message".to_string(), Value::String(message.clone()));
        }
        fields
    }
}

// ── LlmGuardrail ─────────────────────────────────────────────────────────

/// A [`GuardrailCheck`] that uses an LLM to evaluate content against a policy.
///
/// Ports python-sdk's `LLMGuardrail` (`guardrail.py`), which sends the content plus a policy
/// prompt directly to an LLM provider (via `litellm`, bypassing the Conductor server entirely
/// for this one call) and expects a `{"passed": bool, "reason": str}` JSON response, evaluated
/// **synchronously** at check time. This is a genuinely different call path from the rest of
/// this crate's LLM usage: it is not a Conductor `LLM_CHAT_COMPLETE` workflow task, and
/// `AgentRuntime` is not involved — python's own implementation confirms this by calling
/// `litellm.completion(...)` directly, never touching `self._agent_client`.
///
/// Ported for the two providers most of this crate's own examples already use —
/// `"openai/<model>"` (`POST https://api.openai.com/v1/chat/completions`, reading
/// `OPENAI_API_KEY`) and `"anthropic/<model>"` (`POST https://api.anthropic.com/v1/messages`,
/// reading `ANTHROPIC_API_KEY`) — rather than python's full `litellm` multi-provider surface
/// (a dozen-plus providers), which would mean re-implementing a large fraction of `litellm`
/// itself. Any other `"provider/model"` string fails closed with a message naming the
/// unsupported provider, exactly mirroring python's own fail-closed fallback when `litellm`
/// isn't installed (`GuardrailResult(passed=False, message="LLMGuardrail requires the
/// 'litellm' package. ...")`) rather than panicking or silently doing nothing.
///
/// [`GuardrailCheck::check`] is synchronous (matching python, and the rest of this trait), but
/// the HTTP call underneath is necessarily async (this crate's only HTTP client, `reqwest`, is
/// async-only). Bridging that safely — without risking reqwest's classic "can't create a
/// runtime inside a runtime" panic when `check()` is called from within this crate's own
/// multi-threaded Tokio executor (e.g. from a guardrail-worker `Worker::execute`, which is
/// exactly the context the trait's own doc comment anticipates) — means never calling
/// `Handle::block_on` on the *calling* thread. Instead [`run_blocking`] spawns a fresh OS thread
/// with its own throwaway single-threaded runtime for just this one call, and the calling
/// thread only does an ordinary, runtime-agnostic blocking channel `join()` — safe from any
/// context, sync or async, single- or multi-threaded.
#[derive(Debug, Clone)]
pub struct LlmGuardrail {
    model: String,
    policy: String,
    max_tokens: Option<u32>,
}

impl LlmGuardrail {
    /// `model` is a `"provider/model"` string (e.g. `"anthropic/claude-sonnet-4-6"`); `policy`
    /// describes what the guardrail should check for.
    pub fn new(model: impl Into<String>, policy: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            policy: policy.into(),
            max_tokens: None,
        }
    }

    /// Cap the evaluation call's response length.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// The `"provider/model"` string this guardrail evaluates against.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The policy description this guardrail checks content against.
    pub fn policy(&self) -> &str {
        &self.policy
    }
}

/// The exact evaluation prompt python-sdk's `LLMGuardrail._evaluate` sends — same wording, same
/// structure, so a policy tuned against python's prompt behaves identically here.
fn llm_guardrail_prompt(policy: &str, content: &str) -> String {
    format!(
        "You are a content safety evaluator. Evaluate the following content against this policy:\n\nPOLICY: {policy}\n\nCONTENT: {content}\n\nRespond with ONLY a JSON object: {{\"passed\": true/false, \"reason\": \"...\"}}"
    )
}

/// Parse an LLM's raw response text into a [`GuardrailResult`], matching python's exact
/// `_evaluate` parsing: a strict `json.loads` (no fenced-code-block stripping, no leniency
/// beyond what `serde_json` itself tolerates) of `{"passed": ..., "reason": ...}`, defaulting
/// `passed` to `false` and `reason` to `""` if either key is absent. An unparseable response
/// fails closed with the first 200 characters of the raw text, matching python's
/// `result_text[:200]`.
fn parse_llm_guardrail_response(result_text: &str) -> GuardrailResult {
    match serde_json::from_str::<Value>(result_text.trim()) {
        Ok(Value::Object(data)) => {
            let passed = data.get("passed").and_then(Value::as_bool).unwrap_or(false);
            let reason = data
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if passed {
                GuardrailResult::pass()
            } else {
                GuardrailResult::fail(reason)
            }
        }
        _ => {
            let truncated: String = result_text.chars().take(200).collect();
            GuardrailResult::fail(format!(
                "LLM guardrail returned unparseable response: {truncated}"
            ))
        }
    }
}

/// Build the OpenAI Chat Completions request body for one evaluation call.
fn openai_request_body(model: &str, prompt: &str, max_tokens: Option<u32>) -> Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0,
    });
    if let Some(max_tokens) = max_tokens {
        body["max_tokens"] = Value::from(max_tokens);
    }
    body
}

/// Extract the assistant's reply text from an OpenAI Chat Completions response body.
fn extract_openai_content(response: &Value) -> Option<String> {
    response
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(str::to_string)
}

/// Build the Anthropic Messages request body for one evaluation call. Anthropic's API requires
/// `max_tokens` (unlike OpenAI's, where it's optional) — python's `litellm` supplies a default
/// when the caller didn't set one; this does the same (`1024`, litellm's own default for this
/// call shape).
fn anthropic_request_body(model: &str, prompt: &str, max_tokens: Option<u32>) -> Value {
    serde_json::json!({
        "model": model,
        "max_tokens": max_tokens.unwrap_or(1024),
        "temperature": 0,
        "messages": [{"role": "user", "content": prompt}],
    })
}

/// Extract the assistant's reply text from an Anthropic Messages response body.
fn extract_anthropic_content(response: &Value) -> Option<String> {
    response
        .get("content")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(str::to_string)
}

/// Run `future` to completion on a dedicated OS thread with its own throwaway single-threaded
/// Tokio runtime, blocking the *calling* thread on an ordinary channel `recv` — not on
/// `Handle::block_on`. See [`LlmGuardrail`]'s doc comment for why this indirection exists: it's
/// the only way to safely call async `reqwest` code from a synchronous [`GuardrailCheck::check`]
/// that might itself already be running on this crate's multi-threaded async runtime.
/// Runs `future` on its dedicated thread; `Err` covers both ways that thread can fail to
/// deliver an output (couldn't build its own runtime, or panicked before sending) — both
/// collapse into a plain `String` here since callers already treat every failure path as a
/// fail-closed [`GuardrailResult`], not a distinguishable error type.
fn run_blocking<F>(future: F) -> std::result::Result<F::Output, String>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = tx.send(Err(format!(
                    "failed to build LlmGuardrail's evaluation runtime: {e}"
                )));
                return;
            }
        };
        let _ = tx.send(Ok(runtime.block_on(future)));
    });
    rx.recv().unwrap_or_else(|_| {
        Err("LlmGuardrail's evaluation thread ended without sending a result".to_string())
    })
}

/// Call the given provider's chat/messages API and return the assistant's raw reply text, or an
/// error message describing what went wrong (missing API key, transport error, non-2xx status,
/// or an unrecognized provider) — every branch is a `String`, never a panic or propagated error,
/// matching python's blanket `except Exception as e: return GuardrailResult(passed=False,
/// message=f"LLM guardrail evaluation error: {e}")`.
async fn call_llm_provider(
    provider: &str,
    model: &str,
    prompt: &str,
    max_tokens: Option<u32>,
) -> std::result::Result<String, String> {
    let client = reqwest::Client::new();

    let (url, api_key_var, body): (&str, &str, Value) = match provider {
        "openai" => (
            "https://api.openai.com/v1/chat/completions",
            "OPENAI_API_KEY",
            openai_request_body(model, prompt, max_tokens),
        ),
        "anthropic" => (
            "https://api.anthropic.com/v1/messages",
            "ANTHROPIC_API_KEY",
            anthropic_request_body(model, prompt, max_tokens),
        ),
        other => {
            return Err(format!(
                "LlmGuardrail currently only supports the 'openai' and 'anthropic' \
                 providers; got '{other}'"
            ));
        }
    };

    let api_key = std::env::var(api_key_var)
        .map_err(|_| format!("LlmGuardrail: {api_key_var} is not set in the environment"))?;

    let headers: Vec<(String, String)> = match provider {
        "openai" => vec![("Authorization".to_string(), format!("Bearer {api_key}"))],
        "anthropic" => vec![
            ("x-api-key".to_string(), api_key),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
        ],
        _ => unreachable!("provider already validated above"),
    };

    let mut request = client.post(url).json(&body);
    for (header, value) in headers {
        request = request.header(header, value);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("LLM guardrail evaluation error: {e}"))?;

    let status = response.status();
    let response_body: Value = response
        .json()
        .await
        .map_err(|e| format!("LLM guardrail evaluation error: {e}"))?;

    if !status.is_success() {
        return Err(format!(
            "LLM guardrail evaluation error: {provider} API returned {status}: {response_body}"
        ));
    }

    let extracted = match provider {
        "openai" => extract_openai_content(&response_body),
        "anthropic" => extract_anthropic_content(&response_body),
        _ => unreachable!("provider already validated above"),
    };

    extracted.ok_or_else(|| {
        format!("LLM guardrail evaluation error: unrecognized {provider} response shape")
    })
}

impl GuardrailCheck for LlmGuardrail {
    fn check(&self, content: &str) -> GuardrailResult {
        let Some((provider, model)) = self.model.split_once('/') else {
            return GuardrailResult::fail(format!(
                "LlmGuardrail model must be \"provider/model\" (e.g. \"openai/gpt-4o\"); got \
                 '{}'",
                self.model
            ));
        };

        let prompt = llm_guardrail_prompt(&self.policy, content);
        let provider = provider.to_string();
        let model = model.to_string();
        let max_tokens = self.max_tokens;

        let result =
            run_blocking(
                async move { call_llm_provider(&provider, &model, &prompt, max_tokens).await },
            );

        match result {
            Ok(Ok(result_text)) => parse_llm_guardrail_response(&result_text),
            Ok(Err(message)) | Err(message) => GuardrailResult::fail(message),
        }
    }

    fn guardrail_type_fields(&self, _name: &str) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert(
            "guardrailType".to_string(),
            Value::String("llm".to_string()),
        );
        fields.insert("model".to_string(), Value::String(self.model.clone()));
        fields.insert("policy".to_string(), Value::String(self.policy.clone()));
        if let Some(max_tokens) = self.max_tokens {
            fields.insert("maxTokens".to_string(), Value::from(max_tokens));
        }
        fields
    }
}

// ── FunctionGuardrail ────────────────────────────────────────────────────

/// A [`GuardrailCheck`] backed by an arbitrary function, matching python-sdk's `@guardrail`-
/// decorated custom-function case (`Guardrail(func=...)` where `func` isn't the `RegexGuardrail`/
/// `LLMGuardrail` bound-method case). Wire `guardrailType: "custom"`, `taskName: <guardrail
/// name>` — [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) polls a task named
/// after the *guardrail itself* for this type, not a name derived from the owning agent (see
/// [`Guardrail::guardrail_type_fields`]'s note on why `name` is threaded through the trait
/// method).
///
/// # Example
///
/// ```
/// use conductor::agents::{FunctionGuardrail, Guardrail, GuardrailResult};
///
/// let no_pii = Guardrail::new(
///     "no_pii",
///     FunctionGuardrail::new(|content: &str| {
///         if content.contains('@') {
///             GuardrailResult::fail("Response must not contain email addresses.")
///         } else {
///             GuardrailResult::pass()
///         }
///     }),
/// );
/// assert!(!no_pii.check("email me at a@b.com").passed);
/// ```
pub struct FunctionGuardrail {
    check_fn: Arc<dyn Fn(&str) -> GuardrailResult + Send + Sync>,
}

impl FunctionGuardrail {
    pub fn new<F>(check_fn: F) -> Self
    where
        F: Fn(&str) -> GuardrailResult + Send + Sync + 'static,
    {
        Self {
            check_fn: Arc::new(check_fn),
        }
    }
}

impl GuardrailCheck for FunctionGuardrail {
    fn check(&self, content: &str) -> GuardrailResult {
        (self.check_fn)(content)
    }

    fn guardrail_type_fields(&self, name: &str) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert(
            "guardrailType".to_string(),
            Value::String("custom".to_string()),
        );
        fields.insert("taskName".to_string(), Value::String(name.to_string()));
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_and_on_fail_wire_strings() {
        assert_eq!(Position::Input.as_str(), "input");
        assert_eq!(Position::Output.as_str(), "output");
        assert_eq!(OnFail::Retry.as_str(), "retry");
        assert_eq!(OnFail::Raise.as_str(), "raise");
        assert_eq!(OnFail::Fix.as_str(), "fix");
        assert_eq!(OnFail::Human.as_str(), "human");
    }

    #[test]
    fn test_guardrail_result_constructors() {
        let pass = GuardrailResult::pass();
        assert!(pass.passed);
        assert_eq!(pass.message, "");
        assert_eq!(pass.fixed_output, None);

        let fail = GuardrailResult::fail("nope");
        assert!(!fail.passed);
        assert_eq!(fail.message, "nope");
        assert_eq!(fail.fixed_output, None);

        let fixed = GuardrailResult::fail_with_fix("nope", "fixed content");
        assert!(!fixed.passed);
        assert_eq!(fixed.fixed_output, Some("fixed content".to_string()));
    }

    #[test]
    fn test_guardrail_defaults() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        assert_eq!(guardrail.name, "g");
        assert_eq!(guardrail.position, Position::Output);
        assert_eq!(guardrail.on_fail, OnFail::Raise);
        assert_eq!(guardrail.max_retries, 3);
    }

    #[test]
    fn test_with_on_fail_rejects_human_on_input() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker)
            .with_position(Position::Input)
            .unwrap();
        assert!(guardrail.with_on_fail(OnFail::Human).is_err());
    }

    #[test]
    fn test_with_position_rejects_input_when_on_fail_already_human() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker)
            .with_on_fail(OnFail::Human)
            .unwrap();
        assert!(guardrail.with_position(Position::Input).is_err());
    }

    #[test]
    fn test_human_on_output_is_valid() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker).with_on_fail(OnFail::Human);
        assert!(guardrail.is_ok());
    }

    #[test]
    fn test_max_retries_validation() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        assert!(guardrail.with_max_retries(0).is_err());

        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        assert!(guardrail.with_max_retries(5).is_ok());
    }

    #[test]
    fn test_regex_guardrail_rejects_empty_patterns() {
        let empty: Vec<String> = vec![];
        assert!(RegexGuardrail::new(empty).is_err());
    }

    #[test]
    fn test_regex_guardrail_rejects_invalid_pattern() {
        assert!(RegexGuardrail::new(["("]).is_err());
    }

    #[test]
    fn test_regex_guardrail_block_mode_fails_on_match() {
        let guardrail = RegexGuardrail::new([r"[\w.+-]+@[\w-]+\.[\w.-]+"]).unwrap();
        let result = guardrail.check("contact me at a@b.com please");
        assert!(!result.passed);
        assert!(!result.message.is_empty());

        let result = guardrail.check("no contact info here");
        assert!(result.passed);
    }

    #[test]
    fn test_regex_guardrail_allow_mode_fails_when_no_match() {
        let guardrail = RegexGuardrail::new([r"^\s*[\{\[]"])
            .unwrap()
            .with_mode(RegexMode::Allow);
        assert!(guardrail.check("{\"ok\": true}").passed);
        assert!(!guardrail.check("not json").passed);
    }

    #[test]
    fn test_regex_guardrail_custom_message() {
        let guardrail = RegexGuardrail::new(["secret"])
            .unwrap()
            .with_message("custom failure message");
        let result = guardrail.check("this has a secret in it");
        assert_eq!(result.message, "custom failure message");
    }

    #[test]
    fn test_regex_guardrail_accessors() {
        let guardrail = RegexGuardrail::new(["a", "b"])
            .unwrap()
            .with_mode(RegexMode::Allow);
        assert_eq!(guardrail.patterns(), &["a".to_string(), "b".to_string()]);
        assert_eq!(guardrail.mode(), RegexMode::Allow);
    }

    #[test]
    fn test_llm_guardrail_fails_closed_for_unsupported_provider() {
        // Deterministic and network-free: an unrecognized provider fails before any env lookup
        // or HTTP call, matching python's fail-closed-with-a-clear-message behavior when
        // `litellm` itself can't run the evaluation.
        let guardrail = LlmGuardrail::new("some_unsupported_provider/foo", "no harmful content");
        let result = guardrail.check("anything");
        assert!(!result.passed);
        assert!(result.message.contains("some_unsupported_provider"));
    }

    #[test]
    fn test_llm_guardrail_fails_closed_for_malformed_model_string() {
        let guardrail = LlmGuardrail::new("not-a-provider-slash-model", "policy");
        let result = guardrail.check("anything");
        assert!(!result.passed);
        assert!(result.message.contains("not-a-provider-slash-model"));
    }

    /// Regression test: a missing API key must fail closed with a clear message, not panic.
    /// Only asserts when `OPENAI_API_KEY` happens to be unset in the test environment, rather
    /// than mutating a real, process-global env var another test/thread might read concurrently.
    #[test]
    fn test_call_llm_provider_fails_closed_when_api_key_missing() {
        let result = run_blocking(async {
            call_llm_provider("openai", "gpt-4o-mini", "prompt", None).await
        });
        // OPENAI_API_KEY may or may not be set in the environment this test happens to run in
        // -- assert on the shape of failure that matters (never panics, never silently
        // succeeds without a key), not on the exact env var's presence.
        if std::env::var("OPENAI_API_KEY").is_err() {
            let err = result.unwrap().unwrap_err();
            assert!(err.contains("OPENAI_API_KEY"));
        }
    }

    #[test]
    fn test_llm_guardrail_prompt_matches_python_wording() {
        let prompt = llm_guardrail_prompt("no harmful content", "hello there");
        assert!(prompt.contains("POLICY: no harmful content"));
        assert!(prompt.contains("CONTENT: hello there"));
        assert!(prompt.contains("Respond with ONLY a JSON object"));
    }

    #[test]
    fn test_openai_request_body_shape() {
        let body = openai_request_body("gpt-4o-mini", "p", Some(64));
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": "p"}],
                "temperature": 0,
                "max_tokens": 64,
            })
        );
    }

    #[test]
    fn test_openai_request_body_omits_max_tokens_when_unset() {
        let body = openai_request_body("gpt-4o-mini", "p", None);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn test_anthropic_request_body_shape() {
        let body = anthropic_request_body("claude-sonnet-4-6", "p", Some(64));
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-sonnet-4-6",
                "max_tokens": 64,
                "temperature": 0,
                "messages": [{"role": "user", "content": "p"}],
            })
        );
    }

    #[test]
    fn test_anthropic_request_body_defaults_max_tokens_when_unset() {
        let body = anthropic_request_body("claude-sonnet-4-6", "p", None);
        assert_eq!(body.get("max_tokens"), Some(&Value::from(1024)));
    }

    #[test]
    fn test_extract_openai_content() {
        let response = serde_json::json!({
            "choices": [{"message": {"content": "hello"}}]
        });
        assert_eq!(extract_openai_content(&response), Some("hello".to_string()));
        assert_eq!(extract_openai_content(&serde_json::json!({})), None);
    }

    #[test]
    fn test_extract_anthropic_content() {
        let response = serde_json::json!({
            "content": [{"type": "text", "text": "hello"}]
        });
        assert_eq!(
            extract_anthropic_content(&response),
            Some("hello".to_string())
        );
        assert_eq!(extract_anthropic_content(&serde_json::json!({})), None);
    }

    #[test]
    fn test_parse_llm_guardrail_response_passing() {
        let result = parse_llm_guardrail_response(r#"{"passed": true, "reason": ""}"#);
        assert!(result.passed);
    }

    #[test]
    fn test_parse_llm_guardrail_response_failing_carries_reason() {
        let result = parse_llm_guardrail_response(r#"{"passed": false, "reason": "too violent"}"#);
        assert!(!result.passed);
        assert_eq!(result.message, "too violent");
    }

    #[test]
    fn test_parse_llm_guardrail_response_defaults_passed_false_when_key_missing() {
        let result = parse_llm_guardrail_response(r#"{"reason": "no passed key"}"#);
        assert!(!result.passed);
    }

    /// Regression test matching python's exact strictness: python's `_evaluate` does a bare
    /// `json.loads` with no fenced-code-block stripping, so a model that ignores "Respond with
    /// ONLY a JSON object" and wraps its answer in ```json fences fails closed here too, not
    /// leniently parsed through.
    #[test]
    fn test_parse_llm_guardrail_response_fails_closed_on_fenced_json() {
        let result = parse_llm_guardrail_response("```json\n{\"passed\": true}\n```");
        assert!(!result.passed);
        assert!(result.message.contains("unparseable"));
    }

    #[test]
    fn test_parse_llm_guardrail_response_fails_closed_on_non_json_text() {
        let result = parse_llm_guardrail_response("Sure, this content looks fine to me!");
        assert!(!result.passed);
        assert!(result.message.contains("unparseable"));
    }

    #[test]
    fn test_parse_llm_guardrail_response_truncates_long_unparseable_text() {
        let long_text = "x".repeat(500);
        let result = parse_llm_guardrail_response(&long_text);
        assert!(!result.passed);
        // 200 chars of `x` plus the surrounding message text -- just assert the raw text got
        // truncated, not the exact total message length.
        assert!(!result.message.contains(&"x".repeat(201)));
    }

    #[test]
    fn test_llm_guardrail_accessors() {
        let guardrail = LlmGuardrail::new("openai/gpt-4o-mini", "policy text").with_max_tokens(64);
        assert_eq!(guardrail.model(), "openai/gpt-4o-mini");
        assert_eq!(guardrail.policy(), "policy text");
    }

    #[test]
    fn test_guardrail_check_delegates_to_checker() {
        let checker = RegexGuardrail::new(["blocked"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        assert!(guardrail.check("this is fine").passed);
        assert!(!guardrail.check("this is blocked").passed);
    }

    #[test]
    fn test_guardrail_usable_as_boxed_trait_object() {
        let checkers: Vec<Box<dyn GuardrailCheck>> = vec![
            Box::new(RegexGuardrail::new(["x"]).unwrap()),
            Box::new(LlmGuardrail::new("m", "p")),
        ];
        for checker in &checkers {
            let _ = checker.check("content");
        }
        assert_eq!(checkers.len(), 2);
    }

    #[test]
    fn test_function_guardrail_passes_through_closure_result() {
        let checker = FunctionGuardrail::new(|content: &str| {
            if content.contains('@') {
                GuardrailResult::fail("no emails")
            } else {
                GuardrailResult::pass()
            }
        });
        assert!(checker.check("hello").passed);
        assert!(!checker.check("a@b.com").passed);
    }

    #[test]
    fn test_function_guardrail_wire_fields_are_custom_with_task_name() {
        let checker = FunctionGuardrail::new(|_: &str| GuardrailResult::pass());
        let guardrail = Guardrail::new("no_pii", checker);
        let fields = guardrail.guardrail_type_fields();
        assert_eq!(
            fields.get("guardrailType"),
            Some(&Value::String("custom".to_string()))
        );
        assert_eq!(
            fields.get("taskName"),
            Some(&Value::String("no_pii".to_string()))
        );
    }

    #[test]
    fn test_function_guardrail_usable_through_guardrail_wrapper() {
        let guardrail = Guardrail::new(
            "no_pii",
            FunctionGuardrail::new(|content: &str| {
                if content.contains('@') {
                    GuardrailResult::fail("no emails")
                } else {
                    GuardrailResult::pass()
                }
            }),
        )
        .with_on_fail(OnFail::Retry)
        .unwrap();
        assert!(!guardrail.check("a@b.com").passed);
        assert_eq!(guardrail.on_fail, OnFail::Retry);
    }
}
