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
//! external-worker references are left to whoever wires `Guardrail` into `AgentConfigSerializer`
//! (a separate follow-up — this file does not touch `AgentDef`, `serializer.rs`, or `mod.rs`).
//! Likewise, python's `@guardrail` decorator (turning a bare function into a named check) has no
//! equivalent here; nothing in the class diagram calls for it, so it's left as a natural future
//! extension rather than guessed at.

use crate::error::{ConductorError, Result};
use regex::Regex;

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
pub struct Guardrail {
    pub name: String,
    pub position: Position,
    pub on_fail: OnFail,
    pub max_retries: u32,
    checker: Box<dyn GuardrailCheck>,
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
            checker: Box::new(checker),
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
                Regex::new(p).map_err(|e| {
                    ConductorError::agent(format!("invalid regex pattern '{p}': {e}"))
                })
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
                let msg = self.message.clone().unwrap_or_else(|| {
                    "Content did not match any allowed pattern.".to_string()
                });
                GuardrailResult::fail(msg)
            }
            _ => GuardrailResult::pass(),
        }
    }
}

// ── LlmGuardrail ─────────────────────────────────────────────────────────

/// A [`GuardrailCheck`] that uses an LLM to evaluate content against a policy.
///
/// Ports python-sdk's `LLMGuardrail` (`guardrail.py`), which sends the content plus a policy
/// prompt to an LLM (via `litellm`) and expects a `{"passed": bool, "reason": str}` JSON
/// response, evaluated **synchronously** at check time.
///
/// This crate has no synchronous LLM-calling client yet — `AgentRuntime`, the type that would own
/// one, doesn't exist in rust-sdk yet (see `docs/agents/parity-plan.md`). So
/// [`LlmGuardrail::check`] fails closed with a message naming the model/policy it would have
/// evaluated against, exactly mirroring python's own fail-closed fallback when `litellm` isn't
/// installed (`GuardrailResult(passed=False, message="LLMGuardrail requires the 'litellm'
/// package. ...")`). Swap in a real call once an LLM client lands, without changing this type's
/// public shape — `model`/`policy`/`max_tokens` already carry everything python's constructor
/// does.
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

impl GuardrailCheck for LlmGuardrail {
    fn check(&self, _content: &str) -> GuardrailResult {
        GuardrailResult::fail(format!(
            "LlmGuardrail '{}' cannot evaluate synchronously: this SDK version has no LLM \
             client wired in yet (no AgentRuntime — see docs/agents/parity-plan.md). \
             Configured policy: {}",
            self.model, self.policy
        ))
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
        let guardrail =
            RegexGuardrail::new([r"[\w.+-]+@[\w-]+\.[\w.-]+"]).unwrap();
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
    fn test_llm_guardrail_fails_closed() {
        let guardrail = LlmGuardrail::new("anthropic/claude-sonnet-4-6", "no harmful content");
        let result = guardrail.check("anything");
        assert!(!result.passed);
        assert!(result.message.contains("anthropic/claude-sonnet-4-6"));
        assert!(result.message.contains("no harmful content"));
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
}
