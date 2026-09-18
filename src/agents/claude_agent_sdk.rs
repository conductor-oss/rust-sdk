// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Bare subprocess + `stream-json` transport for the Anthropic **Claude Agent SDK** (formerly
//! Claude Code SDK) CLI (`claude`), gated behind the `claude-agent-sdk` Cargo feature.
//!
//! # This is black-box passthrough — read this before wiring it into anything
//!
//! Per `docs/agents/README.md` §"Frameworks", the `claude` CLI's own execution loop
//! (planning, tool calls, sub-agent turns) is fundamentally opaque: there is no `AgentDef`/
//! `FrameworkAgent` extraction path for it the way there is for `async-openai`. Quoting
//! `docs/agents/README.md` directly, because this is the one thing every caller of
//! this module needs to internalize: **"content run through `FrameworkAgent` extraction gets
//! Conductor guardrails/termination; content run through a passthrough adapter does not, and
//! never will unless the loop is unwrapped into a real Conductor task."** Everything the `claude`
//! binary does between the moment this module spawns it and the moment it exits happens outside
//! Conductor's view. No [`crate::agents::Guardrail`], no [`crate::agents::TerminationCondition`],
//! no handoff — none of it can see or touch what happens inside that process.
//!
//! # What this module is
//!
//! Purely the transport layer described in `docs/agents/README.md`'s "Claude Agent SDK —
//! passthrough only" section (subprocess + `stream-json` protocol):
//!
//! 1. [`ClaudeAgentSdkOptions`] — a builder for the CLI flags this transport knows how to set.
//! 2. [`build_args`] — a pure, unit-testable function that turns those options (plus a prompt and
//!    a one-shot/streaming-input mode flag) into the exact `Vec<String>` of CLI arguments,
//!    mirroring the vendored `claude_code_sdk/_internal/transport/subprocess_cli.py::_build_command`
//!    ground truth.
//! 3. [`ClaudeAgentSdkQuery`] — spawns the `claude` binary (via `tokio::process::Command`) using
//!    those arguments and hands back a [`ClaudeAgentSdkStream`] over its newline-delimited JSON
//!    stdout.
//!
//! # What this module explicitly is *not*
//!
//! [`push_event_nonblocking`]/[`update_task_progress_nonblocking`]/[`ProgressMetadata`]/
//! [`ProgressThrottle`] give a caller who wraps a [`ClaudeAgentSdkStream`] in their own
//! `impl Worker` composable pieces of python's instrumentation (event push to
//! `/agent/events/{id}`, throttled `IN_PROGRESS` task updates) — but **not** the largest piece:
//! python's `_create_tracking_workflow`/`_inject_tool_task`/`_complete_tool_task_nonblocking`
//! dynamically register and drive a *real* Conductor workflow/task instance per tool call
//! purely so the CLI's progress renders as a visualized DAG in the Conductor UI. That is a
//! separate, larger follow-up (effectively its own subsystem — dynamic workflow/task
//! definition registration, execution start, and per-tool-call task lifecycle management
//! timed to the live event stream), not something this module attempts. There is also no
//! callback/hook-bridging in the python `claude_code_sdk`-hooks sense — this transport parses
//! raw `stream-json` lines rather than driving an SDK with a hook API to bridge in the first
//! place, so [`ProgressMetadata::record_event`] derives the same counters from each event's own
//! shape instead (see its doc comment for what's narrowed as a result).
//!
//! # Event shape: raw `serde_json::Value`, on purpose
//!
//! The vendored `claude_code_sdk` `types.py` / `_internal/message_parser.py` stream-json shapes
//! (`system` init events, `assistant`/`user` message events each wrapping a nested Anthropic
//! Messages-API `content` array, `result` completion events, and more added across CLI versions)
//! are large, CLI-version-dependent, and not part of any versioned wire contract this crate
//! controls — unlike `crate::agents::AgentEvent`, which *is* this crate's own SSE wire format.
//! Modeling them as a closed Rust enum here would silently break every time the `claude` CLI
//! changes its own output. So each stdout line is handed back as a parsed [`serde_json::Value`];
//! callers that want a typed view can layer their own `serde::Deserialize` shape on top.
//!
//! # Credentials
//!
//! This module never reads or mutates the current process's environment. If the `claude` binary
//! needs an API key (e.g. `ANTHROPIC_API_KEY`), resolve it the normal way (e.g.
//! [`crate::agents::Credentials::get`]) and hand the resolved value to
//! [`ClaudeAgentSdkOptions::with_env`], which is applied via `Command::env()` scoped to the
//! spawned child only — exactly the pattern `docs/agents/README.md`'s `gh_create_issue`
//! example uses for `gh`.
//!
//! # Not implemented here: spawning `claude` in tests
//!
//! The `claude` CLI binary is not installed in this crate's build/CI environment. The tests in
//! this module cover only the pure [`build_args`] function; nothing here spawns a real process.

use std::collections::HashMap;
use std::process::Stdio;

use futures::{Stream, StreamExt as _};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::{Child, Command};

use crate::error::{ConductorError, Result};

/// Name of the `claude` CLI binary this transport shells out to.
const CLAUDE_BINARY: &str = "claude";

/// Resolve a short Claude Agent SDK model alias to its full model id, matching python's
/// `conductor.ai.agents.claude_code.resolve_claude_code_model` table and semantics exactly.
///
/// An empty `alias` returns `None`, meaning "let the `claude` CLI pick its own default" -- the
/// same meaning python gives it. Any other string is looked up in the alias table and, if not
/// found there, passed through unchanged (it's assumed to already be a full model id).
#[must_use]
pub fn resolve_claude_code_model(alias: &str) -> Option<String> {
    if alias.is_empty() {
        return None;
    }
    let resolved = match alias {
        "opus" => "claude-opus-4-6",
        "sonnet" => "claude-sonnet-4-6",
        "haiku" => "claude-haiku-4-5",
        other => other,
    };
    Some(resolved.to_owned())
}

/// Builder for the subset of `claude` CLI flags this transport knows how to set.
///
/// Consuming `with_*` methods, matching this crate's builder convention (see
/// `crate::agents::OpenAiAgent` behind the `openai-adapter` feature).
#[derive(Debug, Clone, Default)]
pub struct ClaudeAgentSdkOptions {
    system_prompt: Option<String>,
    model: Option<String>,
    max_turns: Option<u32>,
    permission_mode: Option<String>,
    allowed_tools: Vec<String>,
    env: HashMap<String, String>,
}

impl ClaudeAgentSdkOptions {
    /// Start from defaults (no system prompt, no model override, no turn limit, default
    /// permission mode, no tool allow-list, no extra child-process env vars).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the `--system-prompt <text>` flag.
    #[must_use]
    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    /// Set the `--model <name>` flag. Sets the value exactly as given -- for resolving a short
    /// alias (`"opus"`/`"sonnet"`/`"haiku"`) first, use
    /// [`ClaudeAgentSdkOptions::with_model_alias`] instead.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the `--model <name>` flag from a short alias, resolving it via
    /// [`resolve_claude_code_model`] first (e.g. `"opus"` becomes `"claude-opus-4-6"`).
    ///
    /// An empty alias is a no-op -- it leaves any previously-set model untouched, matching
    /// [`resolve_claude_code_model`]'s `None` meaning "let the `claude` CLI use its own
    /// default" rather than "clear the model."
    #[must_use]
    pub fn with_model_alias(self, alias: impl AsRef<str>) -> Self {
        match resolve_claude_code_model(alias.as_ref()) {
            Some(model) => self.with_model(model),
            None => self,
        }
    }

    /// Set the `--max-turns <n>` flag.
    #[must_use]
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    /// Set the `--permission-mode <mode>` flag (e.g. `"acceptEdits"`, `"bypassPermissions"`).
    #[must_use]
    pub fn with_permission_mode(mut self, permission_mode: impl Into<String>) -> Self {
        self.permission_mode = Some(permission_mode.into());
        self
    }

    /// Set the `--allowedTools <comma,separated>` flag from a list of tool names.
    #[must_use]
    pub fn with_allowed_tools(mut self, allowed_tools: Vec<String>) -> Self {
        self.allowed_tools = allowed_tools;
        self
    }

    /// Add an environment variable that will be set on the spawned `claude` child process only
    /// (via `Command::env()`), never on this process's own environment. Intended for credentials
    /// such as `ANTHROPIC_API_KEY`, resolved beforehand with e.g.
    /// [`crate::agents::Credentials::get`].
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Pure argument-builder: turns [`ClaudeAgentSdkOptions`] (plus an optional one-shot prompt and a
/// streaming-input flag) into the `claude` CLI argument list, mirroring the vendored
/// `claude_code_sdk/_internal/transport/subprocess_cli.py::_build_command` ground truth.
///
/// Does not include the `claude` binary name itself — that's supplied separately to
/// `Command::new` by the spawning code, keeping this function pure and independent of
/// `tokio::process`.
///
/// - `prompt`: the one-shot prompt text. Ignored when `streaming_input` is `true`.
/// - `streaming_input`: `false` for one-shot mode (`--print -- <prompt>` appended, prompt as the
///   final positional arg); `true` for streaming-input mode (`--input-format stream-json` added,
///   no positional prompt).
fn build_args(
    prompt: Option<&str>,
    opts: &ClaudeAgentSdkOptions,
    streaming_input: bool,
) -> Vec<String> {
    let mut args = vec![
        "--output-format".to_owned(),
        "stream-json".to_owned(),
        "--verbose".to_owned(),
    ];

    if let Some(system_prompt) = &opts.system_prompt {
        args.push("--system-prompt".to_owned());
        args.push(system_prompt.clone());
    }
    if let Some(model) = &opts.model {
        args.push("--model".to_owned());
        args.push(model.clone());
    }
    if let Some(max_turns) = opts.max_turns {
        args.push("--max-turns".to_owned());
        args.push(max_turns.to_string());
    }
    if let Some(permission_mode) = &opts.permission_mode {
        args.push("--permission-mode".to_owned());
        args.push(permission_mode.clone());
    }
    if !opts.allowed_tools.is_empty() {
        args.push("--allowedTools".to_owned());
        args.push(opts.allowed_tools.join(","));
    }

    if streaming_input {
        args.push("--input-format".to_owned());
        args.push("stream-json".to_owned());
    } else {
        args.push("--print".to_owned());
        args.push("--".to_owned());
        if let Some(prompt) = prompt {
            args.push(prompt.to_owned());
        }
    }

    args
}

/// A `claude` CLI invocation, built from [`ClaudeAgentSdkOptions`].
///
/// Named after python-sdk's `query()` entry point (`claude_code_sdk.query`) — this is the
/// session/query object a caller builds once from [`ClaudeAgentSdkOptions`], then uses to spawn
/// one or more one-shot `claude` invocations. See the module docs for what this transport does
/// and does not do.
#[derive(Debug, Clone, Default)]
pub struct ClaudeAgentSdkQuery {
    options: ClaudeAgentSdkOptions,
}

impl ClaudeAgentSdkQuery {
    /// Build a query session from the given options.
    #[must_use]
    pub fn new(options: ClaudeAgentSdkOptions) -> Self {
        Self { options }
    }

    /// Spawn `claude` in one-shot mode (`--print -- <prompt>`) and return a stream over its
    /// newline-delimited `stream-json` stdout.
    ///
    /// Any env vars set via [`ClaudeAgentSdkOptions::with_env`] are applied to the spawned child
    /// process only, via `Command::env()` — this never reads or mutates this process's own
    /// environment.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Io`] if spawning the `claude` binary fails (e.g. not found on `PATH`).
    pub fn spawn(&self, prompt: &str) -> Result<ClaudeAgentSdkStream> {
        let args = build_args(Some(prompt), &self.options, false);
        self.spawn_with_args(args)
    }

    /// Spawn `claude` in streaming-input mode (`--input-format stream-json`, no positional
    /// prompt) and return a stream over its newline-delimited `stream-json` stdout.
    ///
    /// Writing turns to the child's stdin (the other half of the streaming-input protocol) is
    /// not implemented by this transport — see the module docs' "What this module explicitly is
    /// not" section. Only the argument-building and stdout-consuming halves are provided today.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Io`] if spawning the `claude` binary fails (e.g. not found on `PATH`).
    pub fn spawn_streaming(&self) -> Result<ClaudeAgentSdkStream> {
        let args = build_args(None, &self.options, true);
        self.spawn_with_args(args)
    }

    fn spawn_with_args(&self, args: Vec<String>) -> Result<ClaudeAgentSdkStream> {
        let mut command = Command::new(CLAUDE_BINARY);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (key, value) in &self.options.env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(ConductorError::Io)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ConductorError::Internal("claude child had no stdout pipe".into()))?;

        let lines = tokio_stream_lines(stdout);
        Ok(ClaudeAgentSdkStream {
            body: lines.boxed(),
            _child: child,
        })
    }
}

/// Turn a piped child's stdout into a stream of parsed `serde_json::Value`, one per
/// newline-delimited JSON line (blank lines are skipped).
fn tokio_stream_lines(
    stdout: tokio::process::ChildStdout,
) -> impl Stream<Item = Result<Value>> + Send + 'static {
    futures::stream::unfold(BufReader::new(stdout), |mut reader| async move {
        loop {
            let mut line = String::new();
            return match reader.read_line(&mut line).await {
                Ok(0) => None,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let parsed = serde_json::from_str(trimmed).map_err(ConductorError::Json);
                    Some((parsed, reader))
                }
                Err(e) => Some((Err(ConductorError::Io(e)), reader)),
            };
        }
    })
}

/// Decoded newline-delimited `stream-json` stdout stream from a spawned `claude` child process.
///
/// Shaped like [`crate::agents::AgentStream`]: a `body` stream field plus an async `next()`.
/// Holds the spawned [`Child`] alive for the stream's lifetime (dropping it would close the
/// piped stdout and orphan/reap the process).
pub struct ClaudeAgentSdkStream {
    body: futures::stream::BoxStream<'static, Result<Value>>,
    _child: Child,
}

impl ClaudeAgentSdkStream {
    /// Fetch the next decoded `stream-json` event (a raw [`serde_json::Value`] — see the module
    /// docs for why this isn't a typed enum), or `None` once `claude` has exited and its stdout
    /// is fully drained.
    pub async fn next(&mut self) -> Option<Result<Value>> {
        self.body.next().await
    }
}

/// Accumulated progress counters for one Claude Agent SDK passthrough run, matching the
/// `metadata` dict python's `_build_conductor_agent_hooks` threads through its SDK hook
/// callbacks (`tool_call_count`, `tool_error_count`, `subagent_count`, `tools_used`,
/// `last_tool_output`).
///
/// Since this transport parses raw `stream-json` lines instead of receiving SDK hook callbacks
/// (there is no SDK here to hook into a python `claude_code_sdk`-style hook API — see the
/// module docs), [`ProgressMetadata::record_event`] derives the same counters directly from
/// each parsed event's own shape (Anthropic Messages-API content blocks:
/// `tool_use`/`tool_result`) instead. **Narrowed from python's version**: `tools_used` here is
/// just tool names, not python's per-call `{tool_name, args, status, start_time, end_time,
/// duration_ms, stdout, stderr}` entries — there is no tool-call-id-keyed before/after pairing
/// (`PreToolUse` start vs. `PostToolUse` finish) here, only running counts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProgressMetadata {
    pub tool_call_count: u32,
    pub tool_error_count: u32,
    pub subagent_count: u32,
    pub tools_used: Vec<String>,
    pub last_tool_output: String,
}

impl ProgressMetadata {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inspect one parsed `stream-json` event and update these counters. A no-op for events
    /// with no `message.content` array (e.g. `system`/`result` events).
    pub fn record_event(&mut self, event: &Value) {
        let Some(content) = event
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    self.tool_call_count += 1;
                    if let Some(name) = block.get("name").and_then(Value::as_str) {
                        if name == "Agent" || name == "Task" {
                            self.subagent_count += 1;
                        }
                        self.tools_used.push(name.to_owned());
                    }
                }
                Some("tool_result") => {
                    if block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        self.tool_error_count += 1;
                    }
                    if let Some(text) = extract_tool_result_text(block) {
                        self.last_tool_output = text;
                    }
                }
                _ => {}
            }
        }
    }
}

/// A `tool_result` content block's `content` field is either a plain string or an array of
/// nested content blocks (Anthropic Messages API allows both) — this extracts text either way.
fn extract_tool_result_text(block: &Value) -> Option<String> {
    match block.get("content") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(items)) => {
            let text: String = items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

/// Fire-and-forget push of one raw event to `/agent/events/{execution_id}`, matching python's
/// `_push_event_nonblocking`. Spawns a background task and only logs (at debug level) — never
/// propagates — a failed push, so a transient event-push failure never disrupts the caller's
/// main loop over [`ClaudeAgentSdkStream`].
pub fn push_event_nonblocking(
    client: crate::client::AgentClient,
    execution_id: String,
    event: Value,
) {
    tokio::spawn(async move {
        if let Err(e) = client.push_event(&execution_id, &event).await {
            tracing::debug!("event push failed (execution_id={execution_id}): {e}");
        }
    });
}

/// Minimum interval between `IN_PROGRESS` task updates, matching python's
/// `_PROGRESS_UPDATE_INTERVAL_S`.
pub const PROGRESS_UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Tracks whether enough time has passed to push another progress update — matching the
/// throttling python's call sites apply before calling `_update_task_progress_nonblocking` at
/// all, pulled into a small reusable helper here instead of being every caller's own job to
/// remember.
#[derive(Debug)]
pub struct ProgressThrottle {
    last_update: Option<tokio::time::Instant>,
    interval: std::time::Duration,
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        Self {
            last_update: None,
            interval: PROGRESS_UPDATE_INTERVAL,
        }
    }
}

impl ProgressThrottle {
    #[must_use]
    pub fn new(interval: std::time::Duration) -> Self {
        Self {
            last_update: None,
            interval,
        }
    }

    /// `true` if this is the first call, or at least `interval` has elapsed since the last call
    /// that returned `true`. Call this immediately before each candidate progress update and
    /// only actually push when it returns `true`.
    pub fn should_update(&mut self) -> bool {
        let now = tokio::time::Instant::now();
        let due = match self.last_update {
            None => true,
            Some(last) => now.duration_since(last) >= self.interval,
        };
        if due {
            self.last_update = Some(now);
        }
        due
    }
}

/// Max characters of `last_tool_output` included in a progress update, matching python's
/// `_PROGRESS_SNIPPET_MAX_CHARS`.
const PROGRESS_SNIPPET_MAX_CHARS: usize = 500;

/// Fire-and-forget `IN_PROGRESS` task update carrying `metadata`, matching python's
/// `_update_task_progress_nonblocking`: lets the server (and any polling clients) see
/// real-time progress from a long-running Claude Agent SDK passthrough worker. `tools_used` is
/// truncated to the last 5 entries and `last_tool_output` to
/// `PROGRESS_SNIPPET_MAX_CHARS` characters, matching python's slice-last-5/truncate behavior.
/// Like [`push_event_nonblocking`], a failed update is only logged, never propagated.
pub fn update_task_progress_nonblocking(
    task_client: crate::client::TaskClient,
    task_id: String,
    execution_id: String,
    metadata: &ProgressMetadata,
) {
    let mut output_data = HashMap::new();
    output_data.insert(
        "tool_call_count".to_owned(),
        Value::from(metadata.tool_call_count),
    );
    output_data.insert(
        "tool_error_count".to_owned(),
        Value::from(metadata.tool_error_count),
    );
    output_data.insert(
        "subagent_count".to_owned(),
        Value::from(metadata.subagent_count),
    );
    let recent_tools: Vec<Value> = metadata
        .tools_used
        .iter()
        .rev()
        .take(5)
        .rev()
        .cloned()
        .map(Value::String)
        .collect();
    output_data.insert("tools_used".to_owned(), Value::Array(recent_tools));
    let snippet: String = metadata
        .last_tool_output
        .chars()
        .take(PROGRESS_SNIPPET_MAX_CHARS)
        .collect();
    output_data.insert("last_tool_output".to_owned(), Value::String(snippet));

    let result = crate::models::TaskResult {
        task_id,
        workflow_instance_id: execution_id.clone(),
        status: crate::models::TaskResultStatus::InProgress,
        output_data,
        ..Default::default()
    };

    tokio::spawn(async move {
        if let Err(e) = task_client.update_task(&result).await {
            tracing::debug!("task progress update failed (execution_id={execution_id}): {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_claude_code_model_resolves_known_aliases() {
        assert_eq!(
            resolve_claude_code_model("opus"),
            Some("claude-opus-4-6".to_owned())
        );
        assert_eq!(
            resolve_claude_code_model("sonnet"),
            Some("claude-sonnet-4-6".to_owned())
        );
        assert_eq!(
            resolve_claude_code_model("haiku"),
            Some("claude-haiku-4-5".to_owned())
        );
    }

    #[test]
    fn test_resolve_claude_code_model_empty_alias_returns_none() {
        assert_eq!(resolve_claude_code_model(""), None);
    }

    #[test]
    fn test_resolve_claude_code_model_passes_through_unknown_alias() {
        assert_eq!(
            resolve_claude_code_model("claude-opus-4-6"),
            Some("claude-opus-4-6".to_owned())
        );
    }

    #[test]
    fn test_with_model_alias_resolves_and_sets_model() {
        let opts = ClaudeAgentSdkOptions::new().with_model_alias("opus");
        assert_eq!(opts.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn test_with_model_alias_empty_leaves_existing_model_untouched() {
        let opts = ClaudeAgentSdkOptions::new()
            .with_model("claude-opus-4-6")
            .with_model_alias("");
        assert_eq!(opts.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn one_shot_minimal_options_appends_print_and_prompt() {
        let opts = ClaudeAgentSdkOptions::new();
        let args = build_args(Some("hello there"), &opts, false);

        assert_eq!(
            args,
            vec![
                "--output-format",
                "stream-json",
                "--verbose",
                "--print",
                "--",
                "hello there",
            ]
        );
    }

    #[test]
    fn one_shot_applies_all_optional_flags_in_order() {
        let opts = ClaudeAgentSdkOptions::new()
            .with_system_prompt("Be terse.")
            .with_model("claude-opus-5")
            .with_max_turns(3)
            .with_permission_mode("acceptEdits")
            .with_allowed_tools(vec!["Read".to_owned(), "Bash".to_owned()]);

        let args = build_args(Some("do the thing"), &opts, false);

        assert_eq!(
            args,
            vec![
                "--output-format",
                "stream-json",
                "--verbose",
                "--system-prompt",
                "Be terse.",
                "--model",
                "claude-opus-5",
                "--max-turns",
                "3",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "Read,Bash",
                "--print",
                "--",
                "do the thing",
            ]
        );
    }

    #[test]
    fn streaming_input_mode_omits_print_and_prompt() {
        let opts = ClaudeAgentSdkOptions::new().with_model("claude-opus-5");
        let args = build_args(None, &opts, true);

        assert_eq!(
            args,
            vec![
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "claude-opus-5",
                "--input-format",
                "stream-json",
            ]
        );
        assert!(!args.contains(&"--print".to_owned()));
    }

    #[test]
    fn streaming_input_mode_ignores_a_prompt_if_one_is_passed() {
        let opts = ClaudeAgentSdkOptions::new();
        let args = build_args(Some("ignored"), &opts, true);

        assert!(!args.contains(&"ignored".to_owned()));
        assert!(args.contains(&"--input-format".to_owned()));
    }

    #[test]
    fn empty_allowed_tools_omits_the_flag() {
        let opts = ClaudeAgentSdkOptions::new();
        let args = build_args(Some("hi"), &opts, false);

        assert!(!args.contains(&"--allowedTools".to_owned()));
    }

    #[test]
    fn with_env_does_not_affect_build_args() {
        // Credential env vars are applied via Command::env() at spawn time, not baked into the
        // CLI argument list.
        let opts = ClaudeAgentSdkOptions::new().with_env("ANTHROPIC_API_KEY", "sk-test");
        let args = build_args(Some("hi"), &opts, false);

        assert!(args.iter().all(|a| !a.contains("sk-test")));
        assert_eq!(
            opts.env.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("sk-test")
        );
    }

    #[test]
    fn options_builder_is_consuming_and_chainable() {
        let opts = ClaudeAgentSdkOptions::new()
            .with_system_prompt("a")
            .with_model("b")
            .with_max_turns(1)
            .with_permission_mode("c")
            .with_allowed_tools(vec!["d".to_owned()]);

        assert_eq!(opts.system_prompt.as_deref(), Some("a"));
        assert_eq!(opts.model.as_deref(), Some("b"));
        assert_eq!(opts.max_turns, Some(1));
        assert_eq!(opts.permission_mode.as_deref(), Some("c"));
        assert_eq!(opts.allowed_tools, vec!["d".to_owned()]);
    }

    #[test]
    fn test_record_event_counts_tool_use() {
        let mut metadata = ProgressMetadata::new();
        metadata.record_event(&serde_json::json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "name": "Bash", "id": "t1", "input": {}}]},
        }));
        assert_eq!(metadata.tool_call_count, 1);
        assert_eq!(metadata.tools_used, vec!["Bash".to_owned()]);
        assert_eq!(metadata.subagent_count, 0);
    }

    #[test]
    fn test_record_event_counts_subagent_tool_use() {
        let mut metadata = ProgressMetadata::new();
        metadata.record_event(&serde_json::json!({
            "message": {"content": [{"type": "tool_use", "name": "Agent", "id": "t1", "input": {}}]},
        }));
        assert_eq!(metadata.subagent_count, 1);
    }

    #[test]
    fn test_record_event_counts_tool_error() {
        let mut metadata = ProgressMetadata::new();
        metadata.record_event(&serde_json::json!({
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "boom", "is_error": true}]},
        }));
        assert_eq!(metadata.tool_error_count, 1);
        assert_eq!(metadata.last_tool_output, "boom");
    }

    #[test]
    fn test_record_event_non_error_tool_result_does_not_increment_error_count() {
        let mut metadata = ProgressMetadata::new();
        metadata.record_event(&serde_json::json!({
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]},
        }));
        assert_eq!(metadata.tool_error_count, 0);
        assert_eq!(metadata.last_tool_output, "ok");
    }

    #[test]
    fn test_record_event_ignores_events_without_message_content() {
        let mut metadata = ProgressMetadata::new();
        metadata.record_event(&serde_json::json!({"type": "system", "subtype": "init"}));
        assert_eq!(metadata, ProgressMetadata::new());
    }

    #[test]
    fn test_extract_tool_result_text_handles_array_content() {
        let block = serde_json::json!({
            "content": [{"type": "text", "text": "hello"}, {"type": "text", "text": " world"}],
        });
        assert_eq!(
            extract_tool_result_text(&block),
            Some("hello world".to_owned())
        );
    }

    #[test]
    fn test_extract_tool_result_text_handles_string_content() {
        let block = serde_json::json!({"content": "plain text"});
        assert_eq!(
            extract_tool_result_text(&block),
            Some("plain text".to_owned())
        );
    }

    #[test]
    fn test_extract_tool_result_text_none_when_missing() {
        let block = serde_json::json!({});
        assert_eq!(extract_tool_result_text(&block), None);
    }

    #[test]
    fn test_progress_throttle_first_call_is_always_due() {
        let mut throttle = ProgressThrottle::new(std::time::Duration::from_secs(30));
        assert!(throttle.should_update());
    }

    #[test]
    fn test_progress_throttle_immediate_second_call_is_not_due() {
        let mut throttle = ProgressThrottle::new(std::time::Duration::from_secs(30));
        assert!(throttle.should_update());
        assert!(!throttle.should_update());
    }

    #[test]
    fn test_progress_throttle_due_again_after_interval_elapses() {
        let mut throttle = ProgressThrottle::new(std::time::Duration::from_millis(1));
        assert!(throttle.should_update());
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(throttle.should_update());
    }
}
