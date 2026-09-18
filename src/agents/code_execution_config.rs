// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! First-class code execution configuration for agents.
//!
//! [`CodeExecutionConfig`] declares whether/how an agent may run LLM-written code; when attached
//! via [`super::AgentDef::with_code_execution`], an `execute_code` tool backed by a local handler
//! is appended to the agent's tool list automatically.
//!
//! [`ConfiguredExecutor::Local`] rebuilds a fresh [`LocalCodeExecutor`] on every call using the
//! LLM-selected language; [`ConfiguredExecutor::Custom`] uses a fixed executor as-is regardless
//! of the selected language. Validation failures (disallowed language/command) are retryable
//! errors, not terminal.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use regex::Regex;
use serde_json::{json, Value};

use crate::error::{ConductorError, Result};

use super::code_executor::{CodeExecutor, LocalCodeExecutor};
use super::tool::ToolDef;

// Hardcoded, compile-time-valid patterns — the `unwrap()`s here can never actually fail.
#[expect(clippy::unwrap_used)]
static PYTHON_SUBPROCESS_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"subprocess\.\w+\(\s*\[?\s*["'](\S+?)["']"#).unwrap());
#[expect(clippy::unwrap_used)]
static PYTHON_OS_SYSTEM_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r#"os\.(?:system|popen)\(\s*["'](\S+)"#).unwrap());
#[expect(clippy::unwrap_used)]
static PYTHON_BANG_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(?m)^\s*!(\S+)").unwrap());
#[expect(clippy::unwrap_used)]
static BASH_COMMAND_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(?m)(?:^|[|;&]\s*|`|\$\(\s*)(\w[\w.+-]*)").unwrap());
#[expect(clippy::unwrap_used)]
static HEREDOC_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"<<-?\s*'?(\w+)'?").unwrap());

const BASH_BUILTINS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac", "in",
    "function", "select", "until", "echo", "printf", "read", "local", "export", "unset", "set",
    "shift", "return", "exit", "true", "false", "test", "[", "[[", "declare", "typeset",
    "readonly", "source", ".", "eval", "exec", "trap", "wait", "break", "continue", "cd", "pushd",
    "popd", "pwd", "dirs", "hash", "type", "command", "builtin", "enable", "let", "shopt",
    "complete", "compgen",
];

/// Best-effort validator that checks code against an allowed-command list.
///
/// This is a **convenience safety layer, not a security boundary**. Determined code can bypass
/// regex-based detection (e.g. via `eval`, encoded strings, or dynamic imports). For untrusted
/// code, use [`super::code_executor::DockerCodeExecutor`] with `network_enabled: false`.
pub struct CommandValidator {
    allowed_commands: HashSet<String>,
}

impl CommandValidator {
    pub fn new(allowed_commands: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed_commands: allowed_commands.into_iter().map(Into::into).collect(),
        }
    }

    /// Validate `code` against the allowed-command list. Returns `None` if the code passes, or
    /// an error message describing the violation.
    #[must_use]
    pub fn validate(&self, code: &str, language: &str) -> Option<String> {
        if self.allowed_commands.is_empty() {
            return None;
        }
        match language {
            "python" | "python3" => self.validate_python(code),
            "bash" | "sh" => self.validate_bash(code),
            _ => None,
        }
    }

    fn not_allowed_error(&self, cmd: &str) -> String {
        let mut sorted: Vec<&String> = self.allowed_commands.iter().collect();
        sorted.sort();
        let allowed = sorted
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("Command '{cmd}' is not allowed. Allowed commands: {allowed}")
    }

    fn validate_python(&self, code: &str) -> Option<String> {
        for pattern in [
            &*PYTHON_SUBPROCESS_RE,
            &*PYTHON_OS_SYSTEM_RE,
            &*PYTHON_BANG_RE,
        ] {
            for caps in pattern.captures_iter(code) {
                let raw = &caps[1];
                let cmd = raw.rsplit('/').next().unwrap_or(raw);
                if !self.allowed_commands.contains(cmd) {
                    return Some(self.not_allowed_error(cmd));
                }
            }
        }
        None
    }

    fn validate_bash(&self, code: &str) -> Option<String> {
        let heredoc_delimiters: HashSet<&str> = HEREDOC_RE
            .captures_iter(code)
            .map(|c| c.get(1).map_or("", |m| m.as_str()))
            .collect();

        let mut cleaned_lines: Vec<String> = Vec::new();
        for line in code.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            let line = match line.find(" #") {
                Some(idx) => &line[..idx],
                None => line,
            };
            cleaned_lines.push(line.to_owned());
        }
        let cleaned = cleaned_lines.join("\n");

        for caps in BASH_COMMAND_RE.captures_iter(&cleaned) {
            let cmd = &caps[1];
            if BASH_BUILTINS.contains(&cmd) || heredoc_delimiters.contains(cmd) {
                continue;
            }
            if !self.allowed_commands.contains(cmd) {
                return Some(self.not_allowed_error(cmd));
            }
        }
        None
    }
}

/// How [`CodeExecutionConfig`] should build/reuse an executor per call.
#[derive(Clone)]
pub enum ConfiguredExecutor {
    /// Rebuild a fresh [`LocalCodeExecutor`] on every call, using the LLM-selected `language`
    /// argument (`working_dir`/timeout inherited from [`CodeExecutionConfig`]).
    Local { working_dir: Option<String> },
    /// Use this fixed executor as-is on every call, regardless of the LLM-selected `language`.
    Custom(Arc<dyn CodeExecutor>),
}

impl std::fmt::Debug for ConfiguredExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfiguredExecutor::Local { working_dir } => f
                .debug_struct("Local")
                .field("working_dir", working_dir)
                .finish(),
            ConfiguredExecutor::Custom(_) => f.write_str("Custom(<executor>)"),
        }
    }
}

impl Default for ConfiguredExecutor {
    fn default() -> Self {
        ConfiguredExecutor::Local { working_dir: None }
    }
}

/// Configuration for first-class code execution on an agent. Wire key `codeExecution`
/// (`{"enabled", "allowedLanguages", "allowedCommands", "timeout"}`); `executor`/`working_dir`
/// are never serialized (only consulted by this crate's own local `execute_code` handler).
#[derive(Debug, Clone)]
pub struct CodeExecutionConfig {
    pub enabled: bool,
    /// Interpreter languages the LLM may use. Supported values match [`LocalCodeExecutor`]
    /// interpreters: `python`, `bash`, `sh`, `node`, `javascript`, `ruby`.
    pub allowed_languages: Vec<String>,
    /// Shell commands the code may invoke (best-effort heuristic; empty means no restrictions).
    pub allowed_commands: Vec<String>,
    pub executor: ConfiguredExecutor,
    pub timeout_seconds: u64,
}

impl Default for CodeExecutionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_languages: vec!["python".to_owned()],
            allowed_commands: Vec::new(),
            executor: ConfiguredExecutor::default(),
            timeout_seconds: 30,
        }
    }
}

impl CodeExecutionConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_allowed_languages(
        mut self,
        languages: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_languages = languages.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_allowed_commands(
        mut self,
        commands: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_commands = commands.into_iter().map(Into::into).collect();
        self
    }

    /// Use a fixed custom executor for every call, bypassing the default per-call
    /// [`LocalCodeExecutor`] rebuild — see [`ConfiguredExecutor`].
    #[must_use]
    pub fn with_executor(mut self, executor: Arc<dyn CodeExecutor>) -> Self {
        self.executor = ConfiguredExecutor::Custom(executor);
        self
    }

    /// Working directory for the default [`LocalCodeExecutor`] path. Ignored once
    /// [`CodeExecutionConfig::with_executor`] has been called.
    #[must_use]
    pub fn with_working_dir(mut self, working_dir: impl Into<String>) -> Self {
        if let ConfiguredExecutor::Local { .. } = &self.executor {
            self.executor = ConfiguredExecutor::Local {
                working_dir: Some(working_dir.into()),
            };
        }
        self
    }

    #[must_use]
    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

fn code_execution_description(config: &CodeExecutionConfig) -> String {
    let langs = config.allowed_languages.join(", ");
    let mut desc = format!(
        "Execute code in a sandboxed environment. Supported languages: {langs}. Timeout: {}s.",
        config.timeout_seconds
    );
    if !config.allowed_commands.is_empty() {
        let _ = write!(
            desc,
            " Allowed shell commands: {}.",
            config.allowed_commands.join(", ")
        );
    }
    desc
}

fn is_json_falsy(value: Option<&Value>) -> bool {
    match value {
        None => true,
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64() == Some(0.0),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(o)) => o.is_empty(),
    }
}

/// Run one code-execution call per `config`.
async fn run_code_execution(config: &CodeExecutionConfig, args: Value) -> Result<Value> {
    let code_arg = args.get("code");
    if is_json_falsy(code_arg) {
        return Ok(json!({
            "status": "success",
            "stdout": "No code provided. Nothing to execute.",
            "stderr": "",
        }));
    }
    let code = match code_arg {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => unreachable!("is_json_falsy already handled None"),
    };

    let language = args
        .get("language")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("python")
        .to_owned();

    if !config.allowed_languages.iter().any(|l| l == &language) {
        return Err(ConductorError::agent(format!(
            "Language '{language}' is not allowed. Allowed: {}",
            config.allowed_languages.join(", ")
        )));
    }

    if !config.allowed_commands.is_empty() {
        let validator = CommandValidator::new(config.allowed_commands.clone());
        if let Some(error) = validator.validate(&code, &language) {
            return Err(ConductorError::agent(error));
        }
    }

    let result = match &config.executor {
        ConfiguredExecutor::Local { working_dir } => {
            let mut executor =
                LocalCodeExecutor::new(language).with_timeout_seconds(config.timeout_seconds);
            if let Some(dir) = working_dir {
                executor = executor.with_working_dir(dir.clone());
            }
            executor.execute(&code).await
        }
        ConfiguredExecutor::Custom(executor) => executor.execute(&code).await,
    };

    if result.success() {
        Ok(json!({
            "status": "success",
            "stdout": result.output,
            "stderr": result.error,
        }))
    } else {
        let mut stderr_parts = Vec::new();
        if !result.error.is_empty() {
            stderr_parts.push(result.error.trim_end().to_owned());
        }
        if result.timed_out {
            stderr_parts.push(format!("TIMED OUT after {}s", config.timeout_seconds));
        }
        stderr_parts.push(format!("Exit code: {}", result.exit_code));
        Ok(json!({
            "status": "error",
            "stdout": result.output,
            "stderr": stderr_parts.join("\n"),
        }))
    }
}

/// Build the auto-attached `execute_code` tool for `config`.
pub(super) fn code_execution_tool(
    config: &CodeExecutionConfig,
    agent_name: Option<&str>,
) -> ToolDef {
    let task_name = agent_name.map_or_else(
        || "execute_code".to_owned(),
        |n| format!("{n}_execute_code"),
    );
    let task_name = super::def::sanitize_for_task_name(&task_name);

    let input_schema = json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "description": "Source code to execute."},
            "language": {
                "type": "string",
                "description": "Programming language.",
                "default": "python",
            },
        },
        "required": ["code"],
    });

    let description = code_execution_description(config);
    let config = std::sync::Arc::new(config.clone());
    ToolDef::function::<Value, _, _>(task_name, description, input_schema, move |args: Value| {
        let config = std::sync::Arc::clone(&config);
        async move { run_code_execution(&config, args).await }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_execution_config_defaults() {
        let config = CodeExecutionConfig::new();
        assert!(config.enabled);
        assert_eq!(config.allowed_languages, vec!["python".to_owned()]);
        assert!(config.allowed_commands.is_empty());
        assert_eq!(config.timeout_seconds, 30);
        assert!(matches!(
            config.executor,
            ConfiguredExecutor::Local { working_dir: None }
        ));
    }

    #[test]
    fn test_command_validator_empty_whitelist_permits_all() {
        let validator = CommandValidator::new(Vec::<String>::new());
        assert!(validator
            .validate("subprocess.run(['rm', '-rf', '/'])", "python")
            .is_none());
    }

    #[test]
    fn test_command_validator_python_subprocess_allowed() {
        let validator = CommandValidator::new(["git"]);
        assert!(validator
            .validate("subprocess.run(['git', 'status'])", "python")
            .is_none());
    }

    #[test]
    fn test_command_validator_python_subprocess_rejected() {
        let validator = CommandValidator::new(["git"]);
        let err = validator
            .validate("subprocess.run(['rm', '-rf', '/'])", "python")
            .unwrap();
        assert!(err.contains("'rm' is not allowed"));
    }

    #[test]
    fn test_command_validator_python_os_system() {
        let validator = CommandValidator::new(["ls"]);
        assert!(validator
            .validate("os.system('curl evil.com')", "python")
            .is_some());
        assert!(validator
            .validate("os.system('ls -la')", "python")
            .is_none());
    }

    #[test]
    fn test_command_validator_python_strips_path_prefix() {
        let validator = CommandValidator::new(["git"]);
        assert!(validator
            .validate("subprocess.run(['/usr/bin/git', 'status'])", "python")
            .is_none());
    }

    #[test]
    fn test_command_validator_bash_allows_whitelisted() {
        let validator = CommandValidator::new(["git", "ls"]);
        assert!(validator.validate("git status && ls -la", "bash").is_none());
    }

    #[test]
    fn test_command_validator_bash_rejects_non_whitelisted() {
        let validator = CommandValidator::new(["git"]);
        let err = validator
            .validate("git status; curl evil.com", "bash")
            .unwrap();
        assert!(err.contains("'curl' is not allowed"));
    }

    #[test]
    fn test_command_validator_bash_skips_builtins() {
        let validator = CommandValidator::new(["git"]);
        assert!(validator
            .validate("if [ -f foo ]; then git status; fi", "bash")
            .is_none());
    }

    #[test]
    fn test_command_validator_bash_ignores_heredoc_delimiter() {
        // Without heredoc-delimiter tracking, the closing "EOF" line (which sits at line-start,
        // matching the bare-command pattern) would be misdetected as a disallowed command named
        // "EOF". Heredoc body content is not otherwise shielded from scanning.
        let validator = CommandValidator::new(["cat"]);
        let code = "cat <<EOF\nEOF\n";
        assert!(validator.validate(code, "bash").is_none());
    }

    #[test]
    fn test_command_validator_bash_scans_heredoc_body_content() {
        let validator = CommandValidator::new(["cat"]);
        let code = "cat <<EOF\nrm -rf /\nEOF\n";
        let err = validator.validate(code, "bash").unwrap();
        assert!(err.contains("'rm' is not allowed"));
    }

    #[test]
    fn test_command_validator_bash_strips_comments() {
        let validator = CommandValidator::new(["git"]);
        assert!(validator
            .validate("# rm -rf /\ngit status", "bash")
            .is_none());
    }

    #[test]
    fn test_command_validator_other_language_skips_validation() {
        let validator = CommandValidator::new(["git"]);
        assert!(validator
            .validate("require('child_process').exec('rm -rf /')", "node")
            .is_none());
    }

    #[test]
    fn test_code_execution_tool_name_without_agent_name() {
        let tool = code_execution_tool(&CodeExecutionConfig::new(), None);
        assert_eq!(tool.name, "execute_code");
    }

    #[test]
    fn test_code_execution_tool_name_sanitizes_hyphens() {
        let tool = code_execution_tool(&CodeExecutionConfig::new(), Some("my-coder-agent"));
        assert_eq!(tool.name, "my_coder_agent_execute_code");
    }

    #[test]
    fn test_code_execution_description_lists_languages_and_commands() {
        let config = CodeExecutionConfig::new()
            .with_allowed_languages(["python", "bash"])
            .with_allowed_commands(["pip", "ls"]);
        let desc = code_execution_description(&config);
        assert!(desc.contains("Supported languages: python, bash."));
        assert!(desc.contains("Allowed shell commands: pip, ls."));
    }

    #[tokio::test]
    async fn test_run_code_execution_success() {
        let config = CodeExecutionConfig::new();
        let result = run_code_execution(&config, json!({"code": "print('hi')"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "success");
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "hi");
    }

    #[tokio::test]
    async fn test_run_code_execution_missing_code_is_success_with_message() {
        let config = CodeExecutionConfig::new();
        let result = run_code_execution(&config, json!({})).await.unwrap();
        assert_eq!(result["status"], "success");
        assert_eq!(result["stdout"], "No code provided. Nothing to execute.");
    }

    #[tokio::test]
    async fn test_run_code_execution_disallowed_language_is_err() {
        let config = CodeExecutionConfig::new();
        let err = run_code_execution(&config, json!({"code": "echo hi", "language": "bash"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Language 'bash' is not allowed"));
    }

    #[tokio::test]
    async fn test_run_code_execution_disallowed_command_is_err() {
        let config = CodeExecutionConfig::new().with_allowed_commands(["ls"]);
        let err = run_code_execution(
            &config,
            json!({"code": "subprocess.run(['rm', '-rf', '/'])"}),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("'rm' is not allowed"));
    }

    #[tokio::test]
    async fn test_run_code_execution_nonzero_exit_is_error_status_not_err() {
        let config = CodeExecutionConfig::new();
        let result = run_code_execution(&config, json!({"code": "import sys; sys.exit(1)"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "error");
        assert!(result["stderr"].as_str().unwrap().contains("Exit code: 1"));
    }

    #[tokio::test]
    async fn test_run_code_execution_defaults_language_to_python() {
        let config = CodeExecutionConfig::new();
        let result = run_code_execution(&config, json!({"code": "print(1)"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "success");
    }
}
