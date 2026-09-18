// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! First-class CLI command execution for agents.
//!
//! [`CliConfig`] declares which commands an agent may shell out to; when attached via
//! [`super::AgentDef::with_cli_commands`], a `run_command` tool backed by a local handler is
//! appended to the agent's tool list automatically.
//!
//! `context_key` is supported via [`super::ToolContext`]/[`ToolDef::function_with_context`]:
//! on a successful call, the trimmed stdout (falling back to stderr) is recorded via
//! [`super::ToolContext::set_state`] for later pipeline steps to read back.
//!
//! Timeout/missing-executable/unexpected-IO failures are terminal
//! (`ConductorError::terminal_tool`, mapped to `FAILED_WITH_TERMINAL_ERROR`); whitelist/shell-gate
//! violations stay plain [`crate::error::ConductorError::agent`] (retryable). Shell
//! tokenization/quoting uses the [`shell_words`] crate.

use std::fmt::Write as _;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{ConductorError, Result};

use super::tool::{ToolContext, ToolDef};

/// Configuration for first-class CLI command execution on an agent. Wire key `cliConfig`
/// (`{"enabled", "allowedCommands", "timeout", "allowShell"}`); `working_dir` is intentionally
/// never serialized (only consulted by this crate's own local `run_command` handler).
#[derive(Debug, Clone, PartialEq)]
pub struct CliConfig {
    pub enabled: bool,
    /// Command whitelist (e.g. `["git", "gh"]`). Empty means no restrictions.
    pub allowed_commands: Vec<String>,
    pub timeout_seconds: u64,
    pub working_dir: Option<String>,
    /// Config-level gate: may the LLM pass `shell: true`?
    pub allow_shell: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_commands: Vec::new(),
            timeout_seconds: 30,
            working_dir: None,
            allow_shell: false,
        }
    }
}

impl CliConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_allowed_commands(
        mut self,
        commands: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_commands = commands.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub fn with_working_dir(mut self, working_dir: impl Into<String>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    #[must_use]
    pub fn with_allow_shell(mut self, allow_shell: bool) -> Self {
        self.allow_shell = allow_shell;
        self
    }

    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Return `command`'s executable token: the first shell word, tokenizing a full command line
/// (e.g. `"gh repo list --limit 5"`) the same way a bare executable (`"gh"`) is. Falls back to
/// whitespace splitting if `command` isn't validly quoted.
fn executable_of(command: &str) -> String {
    if command.is_empty() {
        return String::new();
    }
    let tokens = shell_words::split(command)
        .unwrap_or_else(|_| command.split_whitespace().map(String::from).collect());
    tokens
        .into_iter()
        .next()
        .unwrap_or_else(|| command.to_owned())
}

/// Validate `command` against `allowed_commands`: keys off the executable (so `"git"` and
/// `"git status -s"` validate identically), strips any path prefix (`/usr/bin/git` -> `git`)
/// first, and permits everything when the whitelist is empty.
fn validate_cli_command(command: &str, allowed_commands: &[String]) -> Result<()> {
    if allowed_commands.is_empty() {
        return Ok(());
    }
    let exe = executable_of(command);
    let base = std::path::Path::new(&exe)
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or(exe);
    if allowed_commands.iter().any(|c| c == &base) {
        return Ok(());
    }
    let mut sorted = allowed_commands.to_vec();
    sorted.sort();
    Err(ConductorError::agent(format!(
        "Command '{base}' is not allowed. Allowed commands: {}",
        sorted.join(", ")
    )))
}

fn cli_tool_description(config: &CliConfig) -> String {
    let mut desc = format!(
        "Run a CLI command directly. Timeout: {}s.",
        config.timeout_seconds
    );
    if !config.allowed_commands.is_empty() {
        let mut sorted = config.allowed_commands.clone();
        sorted.sort();
        let _ = write!(desc, " Allowed commands: {}.", sorted.join(", "));
    }
    if !config.allow_shell {
        desc.push_str(" Shell mode is disabled \u{2014} do not set shell=True.");
    }
    desc
}

fn build_direct_command(executable: &str, command_args: &[String], cwd: Option<&str>) -> Command {
    let mut cmd = Command::new(executable);
    cmd.args(command_args).stdin(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd
}

#[cfg(unix)]
fn build_shell_command(cmd_str: &str, cwd: Option<&str>) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(cmd_str).stdin(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd
}

#[cfg(windows)]
fn build_shell_command(cmd_str: &str, cwd: Option<&str>) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(cmd_str).stdin(Stdio::null());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd
}

/// Run one CLI command per `config`. If `context_key` is set and non-empty on success, the
/// trimmed stdout (falling back to stderr) is recorded via [`ToolContext::set_state`].
async fn run_cli_command(config: &CliConfig, args: Value, context: &ToolContext) -> Result<Value> {
    let command = match args.get("command").and_then(Value::as_str) {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Ok(json!({
                "status": "error",
                "stdout": "",
                "stderr": "No command provided.",
            }));
        }
    };

    let tokens = match shell_words::split(command) {
        Ok(tokens) => tokens,
        Err(e) => {
            return Ok(json!({
                "status": "error",
                "stdout": "",
                "stderr": format!("Could not parse command: {e}"),
            }));
        }
    };
    if tokens.is_empty() {
        return Ok(json!({
            "status": "error",
            "stdout": "",
            "stderr": "No command provided.",
        }));
    }
    let executable = tokens[0].clone();

    validate_cli_command(&executable, &config.allowed_commands)?;

    let shell = args.get("shell").and_then(Value::as_bool).unwrap_or(false);
    if shell && !config.allow_shell {
        return Err(ConductorError::agent(
            "Shell mode is disabled for this agent. Do not set shell=True.",
        ));
    }

    let extra_args: Vec<String> = match args.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        Some(other) => vec![other.to_string()],
    };

    let mut command_args: Vec<String> = tokens[1..].to_vec();
    command_args.extend(extra_args);

    let cwd = args
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| config.working_dir.clone());

    let mut cmd = if shell {
        let cmd_str = std::iter::once(executable.as_str())
            .chain(command_args.iter().map(String::as_str))
            .map(shell_words::quote)
            .collect::<Vec<_>>()
            .join(" ");
        build_shell_command(&cmd_str, cwd.as_deref())
    } else {
        build_direct_command(&executable, &command_args, cwd.as_deref())
    };

    // Timeout/not-found/IO errors here become terminal (non-retryable) errors, unlike the
    // validation/shell-gate failures above.
    let output = match timeout(Duration::from_secs(config.timeout_seconds), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ConductorError::terminal_tool(format!(
                "Command not found: {command}"
            )));
        }
        Ok(Err(e)) => return Err(ConductorError::terminal_tool(e.to_string())),
        Err(_) => {
            return Err(ConductorError::terminal_tool(format!(
                "Command timed out after {}s",
                config.timeout_seconds
            )));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        if let Some(context_key) = args.get("context_key").and_then(Value::as_str) {
            if !context_key.is_empty() {
                let value = {
                    let trimmed_stdout = stdout.trim();
                    if trimmed_stdout.is_empty() {
                        stderr.trim().to_owned()
                    } else {
                        trimmed_stdout.to_owned()
                    }
                };
                if !value.is_empty() {
                    context.set_state(context_key, Value::String(value));
                }
            }
        }
        Ok(json!({
            "status": "success",
            "exit_code": 0,
            "stdout": stdout,
            "stderr": stderr,
        }))
    } else {
        Ok(json!({
            "status": "error",
            "exit_code": output.status.code().unwrap_or(-1),
            "stdout": stdout,
            "stderr": stderr,
        }))
    }
}

/// Build the auto-attached `run_command` tool for `config`. Task name is
/// `{agent_name}_run_command` (sanitized via [`super::def::sanitize_for_task_name`]) when an
/// agent name is given, else bare `"run_command"`.
pub(super) fn cli_command_tool(config: &CliConfig, agent_name: Option<&str>) -> ToolDef {
    let task_name =
        agent_name.map_or_else(|| "run_command".to_owned(), |n| format!("{n}_run_command"));
    let task_name = super::def::sanitize_for_task_name(&task_name);

    let input_schema = json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The CLI command to run, e.g. 'git status' or 'gh repo list --limit 5'.",
            },
            "args": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Extra arguments, merged after any embedded in `command`.",
            },
            "cwd": {"type": "string", "description": "Working directory override for this call."},
            "shell": {
                "type": "boolean",
                "description": "Run through a shell (only if the agent allows it).",
            },
            "context_key": {
                "type": "string",
                "description": "If set, save this command's output for later pipeline steps. \
                    Well-known keys: repo, branch, working_dir, issue_number, pr_url, commit_sha.",
            },
        },
        "required": ["command"],
    });

    let description = cli_tool_description(config);
    let config = Arc::new(config.clone());
    ToolDef::function_with_context::<Value, _, _>(
        task_name,
        description,
        input_schema,
        move |args: Value, context: ToolContext| {
            let config = Arc::clone(&config);
            async move { run_cli_command(&config, args, &context).await }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_config_defaults() {
        let config = CliConfig::new();
        assert!(config.enabled);
        assert!(config.allowed_commands.is_empty());
        assert_eq!(config.timeout_seconds, 30);
        assert!(config.working_dir.is_none());
        assert!(!config.allow_shell);
    }

    #[test]
    fn test_cli_config_builders() {
        let config = CliConfig::new()
            .with_allowed_commands(["git", "gh"])
            .with_timeout_seconds(60)
            .with_working_dir("/tmp")
            .with_allow_shell(true)
            .with_enabled(false);
        assert!(!config.enabled);
        assert_eq!(config.allowed_commands, vec!["git", "gh"]);
        assert_eq!(config.timeout_seconds, 60);
        assert_eq!(config.working_dir, Some("/tmp".to_owned()));
        assert!(config.allow_shell);
    }

    #[test]
    fn test_executable_of_bare_command() {
        assert_eq!(executable_of("git"), "git");
    }

    #[test]
    fn test_executable_of_full_command_line() {
        assert_eq!(executable_of("gh repo list --limit 5"), "gh");
    }

    #[test]
    fn test_executable_of_empty() {
        assert_eq!(executable_of(""), "");
    }

    #[test]
    fn test_validate_cli_command_empty_whitelist_permits_all() {
        validate_cli_command("anything --flag", &[]).unwrap();
    }

    #[test]
    fn test_validate_cli_command_allows_whitelisted() {
        let allowed = vec!["git".to_owned(), "gh".to_owned()];
        validate_cli_command("git status -s", &allowed).unwrap();
    }

    #[test]
    fn test_validate_cli_command_rejects_non_whitelisted() {
        let allowed = vec!["git".to_owned()];
        let err = validate_cli_command("rm -rf /", &allowed).unwrap_err();
        assert!(err.to_string().contains("'rm' is not allowed"));
        assert!(err.to_string().contains("Allowed commands: git"));
    }

    #[test]
    fn test_validate_cli_command_strips_path_prefix() {
        let allowed = vec!["git".to_owned()];
        validate_cli_command("/usr/bin/git status", &allowed).unwrap();
    }

    #[test]
    fn test_cli_tool_description_lists_allowed_commands_sorted() {
        let config = CliConfig::new().with_allowed_commands(["gh", "curl", "git"]);
        let desc = cli_tool_description(&config);
        assert!(desc.contains("Allowed commands: curl, gh, git."));
        assert!(desc.contains("Shell mode is disabled"));
    }

    #[test]
    fn test_cli_tool_description_omits_shell_warning_when_allowed() {
        let config = CliConfig::new().with_allow_shell(true);
        let desc = cli_tool_description(&config);
        assert!(!desc.contains("Shell mode is disabled"));
    }

    #[test]
    fn test_cli_command_tool_name_without_agent_name() {
        let tool = cli_command_tool(&CliConfig::new(), None);
        assert_eq!(tool.name, "run_command");
    }

    #[test]
    fn test_cli_command_tool_name_with_agent_name() {
        let tool = cli_command_tool(&CliConfig::new(), Some("ops"));
        assert_eq!(tool.name, "ops_run_command");
    }

    #[test]
    fn test_cli_command_tool_name_sanitizes_hyphens() {
        let tool = cli_command_tool(&CliConfig::new(), Some("my-ops-agent"));
        assert_eq!(tool.name, "my_ops_agent_run_command");
    }

    #[tokio::test]
    async fn test_run_cli_command_success() {
        let config = CliConfig::new();
        let result = run_cli_command(
            &config,
            json!({"command": "echo hello"}),
            &ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "success");
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_run_cli_command_with_explicit_args() {
        let config = CliConfig::new();
        let result = run_cli_command(
            &config,
            json!({"command": "echo", "args": ["a", "b"]}),
            &ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "success");
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "a b");
    }

    #[tokio::test]
    async fn test_run_cli_command_nonzero_exit_is_error_status_not_err() {
        let config = CliConfig::new();
        let result = run_cli_command(
            &config,
            json!({"command": "false"}),
            &ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "error");
        assert_ne!(result["exit_code"], 0);
    }

    #[tokio::test]
    async fn test_run_cli_command_missing_command_is_error_status_not_err() {
        let config = CliConfig::new();
        let result = run_cli_command(&config, json!({}), &ToolContext::default())
            .await
            .unwrap();
        assert_eq!(result["status"], "error");
        assert_eq!(result["stderr"], "No command provided.");
    }

    #[tokio::test]
    async fn test_run_cli_command_unparseable_is_error_status_not_err() {
        let config = CliConfig::new();
        let result = run_cli_command(
            &config,
            json!({"command": "echo \"unterminated"}),
            &ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "error");
        assert!(result["stderr"]
            .as_str()
            .unwrap()
            .contains("Could not parse command"));
    }

    #[tokio::test]
    async fn test_run_cli_command_rejects_non_whitelisted_as_err() {
        let config = CliConfig::new().with_allowed_commands(["git"]);
        let err = run_cli_command(
            &config,
            json!({"command": "echo hi"}),
            &ToolContext::default(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[tokio::test]
    async fn test_run_cli_command_shell_disabled_by_default_is_err() {
        let config = CliConfig::new();
        let err = run_cli_command(
            &config,
            json!({"command": "echo hi", "shell": true}),
            &ToolContext::default(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Shell mode is disabled"));
    }

    #[tokio::test]
    async fn test_run_cli_command_shell_allowed_runs_through_shell() {
        let config = CliConfig::new().with_allow_shell(true);
        let result = run_cli_command(
            &config,
            json!({"command": "echo $HOME", "shell": true}),
            &ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "success");
    }

    #[tokio::test]
    async fn test_run_cli_command_not_found_is_err() {
        let config = CliConfig::new();
        let err = run_cli_command(
            &config,
            json!({"command": "definitely-not-a-real-executable-xyz"}),
            &ToolContext::default(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Command not found"));
        assert!(matches!(err, ConductorError::TerminalTool(_)));
    }

    #[tokio::test]
    async fn test_run_cli_command_timeout_is_err() {
        let config = CliConfig::new().with_timeout_seconds(0);
        let err = run_cli_command(
            &config,
            json!({"command": "sleep 5"}),
            &ToolContext::default(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert!(matches!(err, ConductorError::TerminalTool(_)));
    }

    #[tokio::test]
    async fn test_run_cli_command_rejects_non_whitelisted_is_retryable_not_terminal() {
        let config = CliConfig::new().with_allowed_commands(["git"]);
        let err = run_cli_command(
            &config,
            json!({"command": "echo hi"}),
            &ToolContext::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ConductorError::Agent(_)));
    }

    #[tokio::test]
    async fn test_run_cli_command_context_key_stores_stdout() {
        let config = CliConfig::new();
        let ctx = ToolContext::default();
        run_cli_command(
            &config,
            json!({"command": "echo hello", "context_key": "greeting"}),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(
            ctx.get_state("greeting"),
            Some(Value::String("hello".to_owned()))
        );
    }

    #[tokio::test]
    async fn test_run_cli_command_without_context_key_does_not_touch_state() {
        let config = CliConfig::new();
        let ctx = ToolContext::default();
        run_cli_command(&config, json!({"command": "echo hello"}), &ctx)
            .await
            .unwrap();
        assert_eq!(ctx.get_state("greeting"), None);
    }

    #[tokio::test]
    async fn test_run_cli_command_respects_working_dir() {
        let dir = std::env::temp_dir();
        let config = CliConfig::new().with_working_dir(dir.to_string_lossy().to_string());
        let result = run_cli_command(&config, json!({"command": "pwd"}), &ToolContext::default())
            .await
            .unwrap();
        assert_eq!(result["status"], "success");
    }
}
