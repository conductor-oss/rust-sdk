// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Bare subprocess + `stream-json` transport for the Anthropic **Claude Agent SDK** (formerly
//! Claude Code SDK) CLI (`claude`), gated behind the `claude-agent-sdk` Cargo feature.
//!
//! # This is black-box passthrough — read this before wiring it into anything
//!
//! Per `docs/agents/parity-plan.md` §"Frameworks", the `claude` CLI's own execution loop
//! (planning, tool calls, sub-agent turns) is fundamentally opaque: there is no `AgentDef`/
//! `FrameworkAgent` extraction path for it the way there is for `async-openai`. Quoting
//! `docs/agents/framework-support.md` directly, because this is the one thing every caller of
//! this module needs to internalize: **"content run through `FrameworkAgent` extraction gets
//! Conductor guardrails/termination; content run through a passthrough adapter does not, and
//! never will unless the loop is unwrapped into a real Conductor task."** Everything the `claude`
//! binary does between the moment this module spawns it and the moment it exits happens outside
//! Conductor's view. No [`crate::agents::Guardrail`], no [`crate::agents::TerminationCondition`],
//! no handoff — none of it can see or touch what happens inside that process.
//!
//! # What this module is
//!
//! Purely the transport layer described in `docs/agents/parity-plan.md`'s item 4
//! ("Claude Agent SDK / CLI passthrough (subprocess + `stream-json` protocol)"):
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
//! No server-side event-push (`/agent/events/{id}`), no Conductor tracking-workflow, no
//! callback/hook-bridging — the parts of python-sdk's "passthrough" adapter that make the CLI's
//! progress visible to a running Conductor workflow. That is documented, real follow-up work
//! (see `docs/agents/parity-plan.md` item 4 and its "Passthrough" mode description), not
//! something this module attempts. Today this module gives a caller a raw JSON event stream and
//! nothing more; wiring that stream into a Conductor task is the next PR's job.
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
//! spawned child only — exactly the pattern `docs/agents/parity-plan.md`'s `gh_create_issue`
//! example uses for `gh`.
//!
//! # Not implemented here: spawning `claude` in tests
//!
//! The `claude` CLI binary is not installed in this crate's build/CI environment. The tests in
//! this module cover only the pure [`build_args`] function; nothing here spawns a real process.

use std::collections::HashMap;
use std::process::Stdio;

use futures::{Stream, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::{ConductorError, Result};

/// Name of the `claude` CLI binary this transport shells out to.
const CLAUDE_BINARY: &str = "claude";

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
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the `--system-prompt <text>` flag.
    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    /// Set the `--model <name>` flag.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the `--max-turns <n>` flag.
    pub fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    /// Set the `--permission-mode <mode>` flag (e.g. `"acceptEdits"`, `"bypassPermissions"`).
    pub fn with_permission_mode(mut self, permission_mode: impl Into<String>) -> Self {
        self.permission_mode = Some(permission_mode.into());
        self
    }

    /// Set the `--allowedTools <comma,separated>` flag from a list of tool names.
    pub fn with_allowed_tools(mut self, allowed_tools: Vec<String>) -> Self {
        self.allowed_tools = allowed_tools;
        self
    }

    /// Add an environment variable that will be set on the spawned `claude` child process only
    /// (via `Command::env()`), never on this process's own environment. Intended for credentials
    /// such as `ANTHROPIC_API_KEY`, resolved beforehand with e.g.
    /// [`crate::agents::Credentials::get`].
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
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
    ];

    if let Some(system_prompt) = &opts.system_prompt {
        args.push("--system-prompt".to_string());
        args.push(system_prompt.clone());
    }
    if let Some(model) = &opts.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }
    if let Some(max_turns) = opts.max_turns {
        args.push("--max-turns".to_string());
        args.push(max_turns.to_string());
    }
    if let Some(permission_mode) = &opts.permission_mode {
        args.push("--permission-mode".to_string());
        args.push(permission_mode.clone());
    }
    if !opts.allowed_tools.is_empty() {
        args.push("--allowedTools".to_string());
        args.push(opts.allowed_tools.join(","));
    }

    if streaming_input {
        args.push("--input-format".to_string());
        args.push("stream-json".to_string());
    } else {
        args.push("--print".to_string());
        args.push("--".to_string());
        if let Some(prompt) = prompt {
            args.push(prompt.to_string());
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
    pub fn new(options: ClaudeAgentSdkOptions) -> Self {
        Self { options }
    }

    /// Spawn `claude` in one-shot mode (`--print -- <prompt>`) and return a stream over its
    /// newline-delimited `stream-json` stdout.
    ///
    /// Any env vars set via [`ClaudeAgentSdkOptions::with_env`] are applied to the spawned child
    /// process only, via `Command::env()` — this never reads or mutates this process's own
    /// environment.
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

#[cfg(test)]
mod tests {
    use super::*;

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
            .with_allowed_tools(vec!["Read".to_string(), "Bash".to_string()]);

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
        assert!(!args.contains(&"--print".to_string()));
    }

    #[test]
    fn streaming_input_mode_ignores_a_prompt_if_one_is_passed() {
        let opts = ClaudeAgentSdkOptions::new();
        let args = build_args(Some("ignored"), &opts, true);

        assert!(!args.contains(&"ignored".to_string()));
        assert!(args.contains(&"--input-format".to_string()));
    }

    #[test]
    fn empty_allowed_tools_omits_the_flag() {
        let opts = ClaudeAgentSdkOptions::new();
        let args = build_args(Some("hi"), &opts, false);

        assert!(!args.contains(&"--allowedTools".to_string()));
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
            .with_allowed_tools(vec!["d".to_string()]);

        assert_eq!(opts.system_prompt.as_deref(), Some("a"));
        assert_eq!(opts.model.as_deref(), Some("b"));
        assert_eq!(opts.max_turns, Some(1));
        assert_eq!(opts.permission_mode.as_deref(), Some("c"));
        assert_eq!(opts.allowed_tools, vec!["d".to_string()]);
    }
}
