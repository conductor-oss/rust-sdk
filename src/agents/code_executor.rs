// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Code executors — sandboxed environments for running LLM-generated code. Ports python-sdk's
//! `code_executor.py`.
//!
//! - [`LocalCodeExecutor`] — runs code in a local subprocess (no sandbox).
//! - [`DockerCodeExecutor`] — runs code inside a Docker container.
//! - [`ServerlessCodeExecutor`] — POSTs code to a remote execution HTTP endpoint.
//!
//! **Not ported: `JupyterCodeExecutor`.** Python's version talks to a real Jupyter kernel over
//! `jupyter_client`'s ZeroMQ wire protocol, maintaining kernel state (variables/imports) across
//! calls — itself an optional, `jupyter_client`-gated capability in python (raises `ImportError`
//! with an install hint if that package is missing). Reproducing this would mean implementing
//! the Jupyter messaging protocol over ZeroMQ from scratch; no existing crate in this workspace
//! provides it, and there is no partial version of "stateful kernel execution" worth shipping.
//! Deferred entirely rather than half-modeled, matching this crate's practice elsewhere (e.g.
//! MCP dynamic discovery, router/handoff/swarm-transfer worker registration).

use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

/// The result of a code execution, matching python's `ExecutionResult` dataclass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecutionResult {
    pub output: String,
    pub error: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

impl ExecutionResult {
    /// `true` if the execution succeeded (exit code 0, no timeout) — matches python's
    /// `ExecutionResult.success` property.
    pub fn success(&self) -> bool {
        self.exit_code == 0 && !self.timed_out
    }
}

/// A sandboxed code execution environment, matching python's `CodeExecutor` ABC.
#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// Execute `code` and return the result. Never expected to panic/error out of band —
    /// failures (bad interpreter, timeout, nonzero exit) are reported *through*
    /// [`ExecutionResult`], matching python's contract.
    async fn execute(&self, code: &str) -> ExecutionResult;

    /// The configured language, e.g. `"python"`.
    fn language(&self) -> &str;

    /// Max seconds before execution is killed.
    fn timeout_seconds(&self) -> u64;
}

fn local_interpreter(language: &str) -> Option<&'static str> {
    match language {
        "python" | "python3" => Some("python3"),
        "bash" => Some("bash"),
        "sh" => Some("sh"),
        "node" | "javascript" => Some("node"),
        "ruby" => Some("ruby"),
        _ => None,
    }
}

fn local_file_extension(language: &str) -> &'static str {
    match language {
        "python" | "python3" => ".py",
        "bash" | "sh" => ".sh",
        "node" | "javascript" => ".js",
        "ruby" => ".rb",
        _ => ".txt",
    }
}

/// Execute code in a local subprocess — no sandboxing, matching python's `LocalCodeExecutor`.
/// The code runs with the same permissions as this process; use [`DockerCodeExecutor`] for
/// untrusted code.
#[derive(Debug, Clone)]
pub struct LocalCodeExecutor {
    pub language: String,
    pub timeout_seconds: u64,
    pub working_dir: Option<String>,
}

impl LocalCodeExecutor {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            timeout_seconds: 30,
            working_dir: None,
        }
    }

    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_working_dir(mut self, working_dir: impl Into<String>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }
}

#[async_trait]
impl CodeExecutor for LocalCodeExecutor {
    async fn execute(&self, code: &str) -> ExecutionResult {
        if code.is_empty() {
            return ExecutionResult {
                output: "No code provided. Nothing to execute.".to_string(),
                ..Default::default()
            };
        }
        let Some(interpreter) = local_interpreter(&self.language) else {
            return ExecutionResult {
                error: format!("Unsupported language: {}", self.language),
                exit_code: 1,
                ..Default::default()
            };
        };

        let mut tmp_path = std::env::temp_dir();
        tmp_path.push(format!(
            "conductor_code_exec_{}{}",
            Uuid::new_v4().simple(),
            local_file_extension(&self.language)
        ));
        if let Err(e) = tokio::fs::write(&tmp_path, code).await {
            return ExecutionResult {
                error: e.to_string(),
                exit_code: 1,
                ..Default::default()
            };
        }

        let mut cmd = Command::new(interpreter);
        cmd.arg(&tmp_path).stdin(Stdio::null());
        if let Some(dir) = &self.working_dir {
            cmd.current_dir(dir);
        }

        let result = match timeout(Duration::from_secs(self.timeout_seconds), cmd.output()).await {
            Ok(Ok(output)) => ExecutionResult {
                output: String::from_utf8_lossy(&output.stdout).to_string(),
                error: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                timed_out: false,
            },
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => ExecutionResult {
                error: format!("Interpreter not found: {interpreter}"),
                exit_code: 127,
                ..Default::default()
            },
            Ok(Err(e)) => ExecutionResult {
                error: e.to_string(),
                exit_code: 1,
                ..Default::default()
            },
            Err(_) => ExecutionResult {
                error: format!("Execution timed out after {}s", self.timeout_seconds),
                exit_code: -1,
                timed_out: true,
                ..Default::default()
            },
        };

        let _ = tokio::fs::remove_file(&tmp_path).await;
        result
    }

    fn language(&self) -> &str {
        &self.language
    }

    fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }
}

/// Execute code inside a Docker container, matching python's `DockerCodeExecutor`. Provides
/// isolation (no host filesystem/network access by default). Requires Docker installed and the
/// daemon running.
#[derive(Debug, Clone)]
pub struct DockerCodeExecutor {
    pub image: String,
    pub language: String,
    pub timeout_seconds: u64,
    pub network_enabled: bool,
    pub memory_limit: Option<String>,
    pub volumes: HashMap<String, String>,
}

impl Default for DockerCodeExecutor {
    fn default() -> Self {
        Self {
            image: "python:3.12-slim".to_string(),
            language: "python".to_string(),
            timeout_seconds: 30,
            network_enabled: false,
            memory_limit: None,
            volumes: HashMap::new(),
        }
    }
}

impl DockerCodeExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_network_enabled(mut self, network_enabled: bool) -> Self {
        self.network_enabled = network_enabled;
        self
    }

    pub fn with_memory_limit(mut self, memory_limit: impl Into<String>) -> Self {
        self.memory_limit = Some(memory_limit.into());
        self
    }

    pub fn with_volume(
        mut self,
        host_path: impl Into<String>,
        container_path: impl Into<String>,
    ) -> Self {
        self.volumes.insert(host_path.into(), container_path.into());
        self
    }
}

#[async_trait]
impl CodeExecutor for DockerCodeExecutor {
    async fn execute(&self, code: &str) -> ExecutionResult {
        let mut cmd = Command::new("docker");
        cmd.arg("run").arg("--rm");
        if !self.network_enabled {
            cmd.arg("--network=none");
        }
        if let Some(limit) = &self.memory_limit {
            cmd.arg("--memory").arg(limit);
        }
        for (host_path, container_path) in &self.volumes {
            cmd.arg("-v")
                .arg(format!("{host_path}:{container_path}:ro"));
        }
        let interpreter = match self.language.as_str() {
            "python" => "python3",
            "bash" => "bash",
            "node" => "node",
            _ => "python3",
        };
        cmd.arg(&self.image).arg(interpreter).arg("-c").arg(code);
        cmd.stdin(Stdio::null());

        // Extra 10s for container startup, matching python.
        match timeout(Duration::from_secs(self.timeout_seconds + 10), cmd.output()).await {
            Ok(Ok(output)) => ExecutionResult {
                output: String::from_utf8_lossy(&output.stdout).to_string(),
                error: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                timed_out: false,
            },
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => ExecutionResult {
                error: "Docker not found. Install Docker to use DockerCodeExecutor.".to_string(),
                exit_code: 127,
                ..Default::default()
            },
            Ok(Err(e)) => ExecutionResult {
                error: e.to_string(),
                exit_code: 1,
                ..Default::default()
            },
            Err(_) => ExecutionResult {
                error: format!("Docker execution timed out after {}s", self.timeout_seconds),
                exit_code: -1,
                timed_out: true,
                ..Default::default()
            },
        }
    }

    fn language(&self) -> &str {
        &self.language
    }

    fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }
}

/// Execute code via a remote HTTP execution service, matching python's
/// `ServerlessCodeExecutor`'s default `_send_request` implementation (POSTs
/// `{"code","language","timeout"}` as JSON, expects a `{"output"/"stdout", "error"/"stderr",
/// "exit_code"}`-shaped JSON response).
///
/// Python's version is an extensible base class meant to be subclassed for custom protocols;
/// Rust has no such inheritance mechanism. Callers who need a different wire protocol should
/// implement [`CodeExecutor`] directly instead of trying to override a method on this struct.
#[derive(Debug, Clone)]
pub struct ServerlessCodeExecutor {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub language: String,
    pub timeout_seconds: u64,
    pub headers: HashMap<String, String>,
}

impl ServerlessCodeExecutor {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_key: None,
            language: "python".to_string(),
            timeout_seconds: 30,
            headers: HashMap::new(),
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

#[async_trait]
impl CodeExecutor for ServerlessCodeExecutor {
    async fn execute(&self, code: &str) -> ExecutionResult {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_seconds + 5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return ExecutionResult {
                    error: format!("Request failed: {e}"),
                    exit_code: 1,
                    ..Default::default()
                }
            }
        };

        let mut request = client.post(&self.endpoint).json(&json!({
            "code": code,
            "language": self.language,
            "timeout": self.timeout_seconds,
        }));
        for (k, v) in &self.headers {
            request = request.header(k, v);
        }
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        match request.send().await {
            Ok(response) => match response.json::<serde_json::Value>().await {
                Ok(data) => ExecutionResult {
                    output: data
                        .get("output")
                        .or_else(|| data.get("stdout"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    error: data
                        .get("error")
                        .or_else(|| data.get("stderr"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    exit_code: data.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    timed_out: false,
                },
                Err(e) => ExecutionResult {
                    error: e.to_string(),
                    exit_code: 1,
                    ..Default::default()
                },
            },
            Err(e) if e.is_timeout() => ExecutionResult {
                error: format!("Request failed: {e}"),
                exit_code: -1,
                timed_out: true,
                ..Default::default()
            },
            Err(e) => ExecutionResult {
                error: format!("Request failed: {e}"),
                exit_code: 1,
                ..Default::default()
            },
        }
    }

    fn language(&self) -> &str {
        &self.language
    }

    fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result_success_requires_zero_exit_and_no_timeout() {
        assert!(ExecutionResult::default().success());
        assert!(!ExecutionResult {
            exit_code: 1,
            ..Default::default()
        }
        .success());
        assert!(!ExecutionResult {
            timed_out: true,
            ..Default::default()
        }
        .success());
    }

    #[tokio::test]
    async fn test_local_code_executor_runs_python() {
        let executor = LocalCodeExecutor::new("python");
        let result = executor.execute("print('hello')").await;
        assert!(result.success());
        assert_eq!(result.output.trim(), "hello");
    }

    #[tokio::test]
    async fn test_local_code_executor_empty_code_short_circuits() {
        let executor = LocalCodeExecutor::new("python");
        let result = executor.execute("").await;
        assert!(result.success());
        assert_eq!(result.output, "No code provided. Nothing to execute.");
    }

    #[tokio::test]
    async fn test_local_code_executor_unsupported_language() {
        let executor = LocalCodeExecutor::new("cobol");
        let result = executor.execute("PRINT HELLO").await;
        assert!(!result.success());
        assert!(result.error.contains("Unsupported language"));
    }

    #[tokio::test]
    async fn test_local_code_executor_nonzero_exit() {
        let executor = LocalCodeExecutor::new("python");
        let result = executor.execute("import sys; sys.exit(2)").await;
        assert!(!result.success());
        assert_eq!(result.exit_code, 2);
    }

    #[tokio::test]
    async fn test_local_code_executor_timeout() {
        let executor = LocalCodeExecutor::new("python").with_timeout_seconds(0);
        let result = executor.execute("import time; time.sleep(5)").await;
        assert!(result.timed_out);
        assert!(!result.success());
    }

    #[tokio::test]
    async fn test_local_code_executor_bash() {
        let executor = LocalCodeExecutor::new("bash");
        let result = executor.execute("echo hi").await;
        assert!(result.success());
        assert_eq!(result.output.trim(), "hi");
    }
}
