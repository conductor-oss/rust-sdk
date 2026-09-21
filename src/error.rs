// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use thiserror::Error;

/// Result type alias for Conductor operations.
pub type Result<T> = std::result::Result<T, ConductorError>;

/// Main error type for all Conductor SDK operations.
#[derive(Error, Debug)]
pub enum ConductorError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authentication error.
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Task execution error.
    #[error("Task execution error: {0}")]
    TaskExecution(String),

    /// Task not found.
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Workflow not found.
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),

    /// Workflow execution error.
    #[error("Workflow error: {0}")]
    Workflow(String),

    /// Worker error.
    #[error("Worker error: {0}")]
    Worker(String),

    /// Timeout error.
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Server error with status code.
    #[error("Server error ({status}): {message}")]
    Server { status: u16, message: String },

    /// API error with details.
    #[error("API error: {message}")]
    Api {
        message: String,
        code: Option<String>,
    },

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Channel error (for async communication).
    #[error("Channel error: {0}")]
    Channel(String),

    /// Agent definition, tool-definition, or agent-config-serialization error.
    #[cfg(feature = "agents")]
    #[error("Agent error: {0}")]
    Agent(String),

    /// One or more declared credentials were not present in `Task.runtime_metadata`.
    ///
    /// Per `docs/agents/README.md`, credential resolution fails closed: a
    /// name a tool/agent declared but that the server didn't attach to the polled `Task` is
    /// always an error, never a silent fallback to the process environment. Carries only the
    /// missing *names*, never a value.
    #[cfg(feature = "agents")]
    #[error("Required credentials not found: {}", .0.join(", "))]
    CredentialNotFound(Vec<String>),

    /// A tool/worker failure explicitly marked non-retryable, e.g. a timed-out or
    /// missing-executable command. A `ToolHandler` returns this instead of any other error
    /// variant to signal it; the tool-dispatch worker
    /// (`ToolWorker` in `agents/runtime.rs`) recognizes it and reports
    /// `WorkerOutput::FailedWithTerminalError` (`FAILED_WITH_TERMINAL_ERROR`) instead of the
    /// default retryable `Failed`.
    #[cfg(feature = "agents")]
    #[error("Terminal tool error: {0}")]
    TerminalTool(String),

    /// [`crate::agents::AgentHandle::join`] detected a task stuck `SCHEDULED` with no worker
    /// ever polling it, past the configured stall threshold, with `StallPolicy::Raise`
    /// selected. See [`StalledTaskInfo`] and `docs/agents/README.md`'s liveness notes: this
    /// check is workflow-scoped rather than domain-scoped, since this crate has no
    /// per-execution worker domain to scope by.
    #[cfg(feature = "agents")]
    #[error(
        "Worker stall detected on execution {execution_id}: {} task(s) queued with no poller",
        .stalled_tasks.len()
    )]
    WorkerStall {
        execution_id: String,
        stalled_tasks: Vec<StalledTaskInfo>,
    },
}

/// One `SCHEDULED` task that has been queued past the stall threshold with `poll_count == 0`
/// -- i.e. no worker has ever polled for it. Carried by
/// [`ConductorError::WorkerStall`]. See `docs/agents/README.md`'s liveness notes.
#[cfg(feature = "agents")]
#[derive(Debug, Clone, PartialEq)]
pub struct StalledTaskInfo {
    /// The task definition name (task type) that's stuck.
    pub task_def_name: String,
    /// The specific stuck task's ID.
    pub task_id: String,
    /// How long it's been queued, in seconds.
    pub seconds_queued: f64,
}

impl ConductorError {
    /// Create a configuration error.
    pub fn config(msg: impl Into<String>) -> Self {
        ConductorError::Config(msg.into())
    }

    /// Create an authentication error.
    pub fn auth(msg: impl Into<String>) -> Self {
        ConductorError::Auth(msg.into())
    }

    /// Create a task execution error.
    pub fn task_execution(msg: impl Into<String>) -> Self {
        ConductorError::TaskExecution(msg.into())
    }

    /// Create a worker error.
    pub fn worker(msg: impl Into<String>) -> Self {
        ConductorError::Worker(msg.into())
    }

    /// Create an internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        ConductorError::Internal(msg.into())
    }

    /// Create a server error.
    pub fn server(status: u16, message: impl Into<String>) -> Self {
        ConductorError::Server {
            status,
            message: message.into(),
        }
    }

    /// Create an API error.
    pub fn api(message: impl Into<String>, code: Option<String>) -> Self {
        ConductorError::Api {
            message: message.into(),
            code,
        }
    }

    /// Create an agent error.
    #[cfg(feature = "agents")]
    pub fn agent(msg: impl Into<String>) -> Self {
        ConductorError::Agent(msg.into())
    }

    /// Create a terminal (non-retryable) tool error — see [`crate::error::ConductorError::TerminalTool`].
    #[cfg(feature = "agents")]
    pub fn terminal_tool(msg: impl Into<String>) -> Self {
        ConductorError::TerminalTool(msg.into())
    }

    /// Create a "credential not found" error for one or more missing declared credential names.
    #[cfg(feature = "agents")]
    pub fn credential_not_found(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        ConductorError::CredentialNotFound(names.into_iter().map(Into::into).collect())
    }

    /// Check if this error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            ConductorError::Http(e) => {
                e.is_timeout() || e.is_connect() || e.status().is_some_and(|s| s.is_server_error())
            }
            ConductorError::Server { status, .. } => *status >= 500,
            ConductorError::Timeout(_) => true,
            ConductorError::Channel(_) => false,
            _ => false,
        }
    }
}
