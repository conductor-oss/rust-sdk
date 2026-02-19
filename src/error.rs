// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use thiserror::Error;

/// Result type alias for Conductor operations
pub type Result<T> = std::result::Result<T, ConductorError>;

/// Main error type for all Conductor SDK operations
#[derive(Error, Debug)]
pub enum ConductorError {
    /// HTTP request failed
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization/deserialization failed
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Task execution error
    #[error("Task execution error: {0}")]
    TaskExecution(String),

    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Workflow not found
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),

    /// Workflow execution error
    #[error("Workflow error: {0}")]
    Workflow(String),

    /// Worker error
    #[error("Worker error: {0}")]
    Worker(String),

    /// Timeout error
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Server error with status code
    #[error("Server error ({status}): {message}")]
    Server { status: u16, message: String },

    /// API error with details
    #[error("API error: {message}")]
    Api {
        message: String,
        code: Option<String>,
    },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Channel error (for async communication)
    #[error("Channel error: {0}")]
    Channel(String),
}

impl ConductorError {
    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        ConductorError::Config(msg.into())
    }

    /// Create an authentication error
    pub fn auth(msg: impl Into<String>) -> Self {
        ConductorError::Auth(msg.into())
    }

    /// Create a task execution error
    pub fn task_execution(msg: impl Into<String>) -> Self {
        ConductorError::TaskExecution(msg.into())
    }

    /// Create a worker error
    pub fn worker(msg: impl Into<String>) -> Self {
        ConductorError::Worker(msg.into())
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        ConductorError::Internal(msg.into())
    }

    /// Create a server error
    pub fn server(status: u16, message: impl Into<String>) -> Self {
        ConductorError::Server {
            status,
            message: message.into(),
        }
    }

    /// Create an API error
    pub fn api(message: impl Into<String>, code: Option<String>) -> Self {
        ConductorError::Api {
            message: message.into(),
            code,
        }
    }

    /// Check if this error is retryable
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
