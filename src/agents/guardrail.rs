// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Guardrails — input and output validation for agent responses.
//!
//! A [`Guardrail`] wraps a [`GuardrailCheck`] implementation ([`RegexGuardrail`],
//! [`LlmGuardrail`], or [`FunctionGuardrail`]) plus config for where it runs ([`Position`]) and
//! what happens on failure ([`OnFail`]). Guardrails compile to Conductor worker tasks positioned
//! before ([`Position::Input`]) or after ([`Position::Output`]) the `LlmChatComplete` task; on
//! [`OnFail::Retry`] the guardrail's message is appended to the conversation and the LLM is
//! called again.
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
    /// Wire-format string for this variant.
    #[must_use]
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
    /// Wire-format string for this variant.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            OnFail::Retry => "retry",
            OnFail::Raise => "raise",
            OnFail::Fix => "fix",
            OnFail::Human => "human",
        }
    }
}

// `on_fail = Human` is only valid for `position = Output`; input guardrails are client-side
// and cannot pause a workflow.
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
    #[must_use]
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
/// [`RegexGuardrail`], [`LlmGuardrail`], and [`FunctionGuardrail`] are the built-in
/// implementations.
pub trait GuardrailCheck: Send + Sync {
    /// Run the check against `content`.
    fn check(&self, content: &str) -> GuardrailResult;

    /// Type-specific wire fields for [`AgentConfigSerializer`](super::AgentConfigSerializer) —
    /// the `guardrailType` discriminant (`"regex"`, `"llm"`, `"custom"`, ...) plus whatever
    /// fields that concrete guardrail type contributes. `name` is the wrapping [`Guardrail`]'s
    /// name, threaded through because [`FunctionGuardrail`]'s `"custom"` wire shape needs it for
    /// `taskName` — the checker itself has no name of its own.
    fn guardrail_type_fields(&self, name: &str) -> Map<String, Value>;
}

// ── Guardrail ────────────────────────────────────────────────────────────

/// A validation guardrail for agent input or output.
///
/// Wraps a [`GuardrailCheck`] with config for where it runs ([`Guardrail::with_position`]), what
/// happens on failure ([`Guardrail::with_on_fail`]), and how many retries it gets
/// ([`Guardrail::with_max_retries`]).
///
/// Construct with [`Guardrail::new`] and compose with consuming `with_*` builders. Defaults:
/// `position = Output`, `on_fail = Raise`, `max_retries = 3`.
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

    /// Set where the guardrail runs. Rejects `position = Input` together with an already-set
    /// `on_fail = Human`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if this would combine `position = Input` with an already-set `on_fail = Human`.
    pub fn with_position(mut self, position: Position) -> Result<Self> {
        validate_position_on_fail(position, self.on_fail)?;
        self.position = position;
        Ok(self)
    }

    /// Set what to do when the check fails. Rejects `on_fail = Human` combined with
    /// `position = Input` — input guardrails are client-side and cannot pause a workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if this would combine `on_fail = Human` with an already-set `position = Input`.
    pub fn with_on_fail(mut self, on_fail: OnFail) -> Result<Self> {
        validate_position_on_fail(self.position, on_fail)?;
        self.on_fail = on_fail;
        Ok(self)
    }

    /// Set the max retry attempts used when `on_fail = Retry`. Must be at least 1.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `max_retries` is 0.
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
    #[must_use]
    pub fn check(&self, content: &str) -> GuardrailResult {
        self.checker.check(content)
    }

    /// Type-specific wire fields (`guardrailType` plus that type's own fields), delegating to
    /// the wrapped [`GuardrailCheck`]. See [`GuardrailCheck::guardrail_type_fields`].
    #[must_use]
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
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `patterns` is empty, or if any entry is not a valid regex.
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
    #[must_use]
    pub fn with_mode(mut self, mode: RegexMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set a custom failure message (default: an auto-generated one).
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// The original pattern strings this guardrail was constructed with.
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.pattern_strings
    }

    /// The configured match mode.
    #[must_use]
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
                    .unwrap_or_else(|| "Content matched a blocked pattern.".to_owned());
                GuardrailResult::fail(msg)
            }
            (RegexMode::Allow, false) => {
                let msg = self
                    .message
                    .clone()
                    .unwrap_or_else(|| "Content did not match any allowed pattern.".to_owned());
                GuardrailResult::fail(msg)
            }
            _ => GuardrailResult::pass(),
        }
    }

    fn guardrail_type_fields(&self, _name: &str) -> Map<String, Value> {
        let mut fields = Map::new();
        fields.insert(
            "guardrailType".to_owned(),
            Value::String("regex".to_owned()),
        );
        fields.insert(
            "patterns".to_owned(),
            Value::Array(
                self.pattern_strings
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        fields.insert(
            "mode".to_owned(),
            Value::String(
                match self.mode {
                    RegexMode::Block => "block",
                    RegexMode::Allow => "allow",
                }
                .to_owned(),
            ),
        );
        if let Some(message) = &self.message {
            fields.insert("message".to_owned(), Value::String(message.clone()));
        }
        fields
    }
}

// ── LlmGuardrail ─────────────────────────────────────────────────────────

/// A [`GuardrailCheck`] that uses an LLM to evaluate content against a policy.
///
/// Calls the LLM provider's API directly and synchronously — this is not a Conductor
/// `LLM_CHAT_COMPLETE` workflow task, and `AgentRuntime` is not involved. Supports
/// `"openai/<model>"` (reads `OPENAI_API_KEY`) and `"anthropic/<model>"` (reads
/// `ANTHROPIC_API_KEY`); any other `"provider/model"` string fails closed with an error message
/// naming the unsupported provider.
///
/// [`GuardrailCheck::check`] is synchronous, but the underlying HTTP call runs on a dedicated OS
/// thread with its own Tokio runtime, so it is safe to call even from within this crate's own
/// async runtime.
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
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// The `"provider/model"` string this guardrail evaluates against.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The policy description this guardrail checks content against.
    #[must_use]
    pub fn policy(&self) -> &str {
        &self.policy
    }
}

// The evaluation prompt sent to the LLM for a given policy and content.
fn llm_guardrail_prompt(policy: &str, content: &str) -> String {
    format!(
        "You are a content safety evaluator. Evaluate the following content against this policy:\n\nPOLICY: {policy}\n\nCONTENT: {content}\n\nRespond with ONLY a JSON object: {{\"passed\": true/false, \"reason\": \"...\"}}"
    )
}

// Parse an LLM's raw response text into a GuardrailResult. Expects strict JSON
// `{"passed": bool, "reason": string}` (no fenced-code-block stripping); a missing key defaults
// `passed` to `false` and `reason` to `""`. An unparseable response fails closed with the first
// 200 characters of the raw text.
fn parse_llm_guardrail_response(result_text: &str) -> GuardrailResult {
    if let Ok(Value::Object(data)) = serde_json::from_str::<Value>(result_text.trim()) {
        let passed = data.get("passed").and_then(Value::as_bool).unwrap_or(false);
        let reason = data
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if passed {
            GuardrailResult::pass()
        } else {
            GuardrailResult::fail(reason)
        }
    } else {
        let truncated: String = result_text.chars().take(200).collect();
        GuardrailResult::fail(format!(
            "LLM guardrail returned unparseable response: {truncated}"
        ))
    }
}

// Build the OpenAI Chat Completions request body for one evaluation call.
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

// Extract the assistant's reply text from an OpenAI Chat Completions response body.
fn extract_openai_content(response: &Value) -> Option<String> {
    response
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(str::to_owned)
}

// Build the Anthropic Messages request body for one evaluation call. Anthropic requires
// `max_tokens`; defaults to `1024` if not set.
fn anthropic_request_body(model: &str, prompt: &str, max_tokens: Option<u32>) -> Value {
    serde_json::json!({
        "model": model,
        "max_tokens": max_tokens.unwrap_or(1024),
        "temperature": 0,
        "messages": [{"role": "user", "content": prompt}],
    })
}

// Extract the assistant's reply text from an Anthropic Messages response body.
fn extract_anthropic_content(response: &Value) -> Option<String> {
    response
        .get("content")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(str::to_owned)
}

// Run `future` to completion on a dedicated OS thread with its own single-threaded Tokio
// runtime, blocking the calling thread on a channel `recv` rather than `Handle::block_on` --
// safe to call even from within an existing async runtime. `Err` covers both ways the thread
// can fail to deliver a result (couldn't build its own runtime, or panicked before sending).
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
        Err("LlmGuardrail's evaluation thread ended without sending a result".to_owned())
    })
}

// Call the given provider's chat/messages API and return the assistant's raw reply text, or an
// error message describing what went wrong (missing API key, transport error, non-2xx status,
// or an unrecognized provider).
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
        .map_err(|_var_err| format!("LlmGuardrail: {api_key_var} is not set in the environment"))?;

    let headers: Vec<(String, String)> = match provider {
        "openai" => vec![("Authorization".to_owned(), format!("Bearer {api_key}"))],
        "anthropic" => vec![
            ("x-api-key".to_owned(), api_key),
            ("anthropic-version".to_owned(), "2023-06-01".to_owned()),
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
        let provider = provider.to_owned();
        let model = model.to_owned();
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
        fields.insert("guardrailType".to_owned(), Value::String("llm".to_owned()));
        fields.insert("model".to_owned(), Value::String(self.model.clone()));
        fields.insert("policy".to_owned(), Value::String(self.policy.clone()));
        if let Some(max_tokens) = self.max_tokens {
            fields.insert("maxTokens".to_owned(), Value::from(max_tokens));
        }
        fields
    }
}

// ── FunctionGuardrail ────────────────────────────────────────────────────

/// A [`GuardrailCheck`] backed by an arbitrary function. Wire `guardrailType: "custom"`,
/// `taskName: <guardrail name>` — [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve)
/// polls a task named after the guardrail itself for this type.
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
            "guardrailType".to_owned(),
            Value::String("custom".to_owned()),
        );
        fields.insert("taskName".to_owned(), Value::String(name.to_owned()));
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
        assert_eq!(fixed.fixed_output, Some("fixed content".to_owned()));
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
        guardrail.with_on_fail(OnFail::Human).unwrap_err();
    }

    #[test]
    fn test_with_position_rejects_input_when_on_fail_already_human() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker)
            .with_on_fail(OnFail::Human)
            .unwrap();
        guardrail.with_position(Position::Input).unwrap_err();
    }

    #[test]
    fn test_human_on_output_is_valid() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker).with_on_fail(OnFail::Human);
        guardrail.unwrap();
    }

    #[test]
    fn test_max_retries_validation() {
        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        guardrail.with_max_retries(0).unwrap_err();

        let checker = RegexGuardrail::new(["x"]).unwrap();
        let guardrail = Guardrail::new("g", checker);
        guardrail.with_max_retries(5).unwrap();
    }

    #[test]
    fn test_regex_guardrail_rejects_empty_patterns() {
        let empty: Vec<String> = vec![];
        RegexGuardrail::new(empty).unwrap_err();
    }

    #[test]
    fn test_regex_guardrail_rejects_invalid_pattern() {
        RegexGuardrail::new(["("]).unwrap_err();
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
        assert_eq!(guardrail.patterns(), &["a".to_owned(), "b".to_owned()]);
        assert_eq!(guardrail.mode(), RegexMode::Allow);
    }

    #[test]
    fn test_llm_guardrail_fails_closed_for_unsupported_provider() {
        // Deterministic and network-free: an unrecognized provider fails before any env lookup
        // or HTTP call.
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

    // Regression test: a missing API key must fail closed with a clear message, not panic.
    // Only asserts when `OPENAI_API_KEY` happens to be unset in the test environment, rather
    // than mutating a real, process-global env var another test/thread might read concurrently.
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
    fn test_llm_guardrail_prompt_matches_recorded_wording() {
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
        assert_eq!(extract_openai_content(&response), Some("hello".to_owned()));
        assert_eq!(extract_openai_content(&serde_json::json!({})), None);
    }

    #[test]
    fn test_extract_anthropic_content() {
        let response = serde_json::json!({
            "content": [{"type": "text", "text": "hello"}]
        });
        assert_eq!(
            extract_anthropic_content(&response),
            Some("hello".to_owned())
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

    // Regression test: a model that wraps its JSON answer in triple-backtick fences fails
    // closed rather than being leniently parsed.
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
            Some(&Value::String("custom".to_owned()))
        );
        assert_eq!(
            fields.get("taskName"),
            Some(&Value::String("no_pii".to_owned()))
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
