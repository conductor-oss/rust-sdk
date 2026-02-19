// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::env;
use std::time::Duration;

/// Worker configuration settings
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Task definition name
    pub task_definition_name: String,

    /// Polling interval
    pub poll_interval: Duration,

    /// Worker domain for task routing
    pub domain: Option<String>,

    /// Unique worker identifier
    pub worker_id: String,

    /// Maximum concurrent task executions
    pub thread_count: usize,

    /// Auto-register task definition on startup
    pub register_task_def: bool,

    /// Overwrite existing task definitions when registering
    pub overwrite_task_def: bool,

    /// Enforce strict schema validation (additionalProperties=false)
    pub strict_schema: bool,

    /// Poll timeout
    pub poll_timeout: Duration,

    /// Whether the worker is paused
    pub paused: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            task_definition_name: String::new(),
            poll_interval: Duration::from_millis(100),
            domain: None,
            worker_id: generate_worker_id(),
            thread_count: 1,
            register_task_def: false,
            overwrite_task_def: true,
            strict_schema: false,
            poll_timeout: Duration::from_millis(100),
            paused: false,
        }
    }
}

impl WorkerConfig {
    /// Create a new worker config with the given task name
    pub fn new(task_definition_name: impl Into<String>) -> Self {
        Self {
            task_definition_name: task_definition_name.into(),
            ..Default::default()
        }
    }

    /// Set poll interval
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set poll interval in milliseconds
    pub fn with_poll_interval_millis(mut self, millis: u64) -> Self {
        self.poll_interval = Duration::from_millis(millis);
        self
    }

    /// Set domain
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Set worker ID
    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = worker_id.into();
        self
    }

    /// Set thread count (max concurrent executions)
    pub fn with_thread_count(mut self, count: usize) -> Self {
        self.thread_count = count;
        self
    }

    /// Enable task definition registration
    pub fn with_register_task_def(mut self, register: bool) -> Self {
        self.register_task_def = register;
        self
    }

    /// Set poll timeout
    pub fn with_poll_timeout(mut self, timeout: Duration) -> Self {
        self.poll_timeout = timeout;
        self
    }

    /// Enable strict schema validation
    pub fn with_strict_schema(mut self, strict: bool) -> Self {
        self.strict_schema = strict;
        self
    }
}

/// Resolve worker configuration from environment variables
///
/// This follows the Python SDK's hierarchical configuration pattern:
/// 1. Worker-specific env: `CONDUCTOR_WORKER_{WORKER_NAME}_{PROPERTY}`
/// 2. Global env: `CONDUCTOR_WORKER_ALL_{PROPERTY}`
/// 3. Code defaults
pub fn resolve_worker_config(worker_name: &str, defaults: WorkerConfig) -> WorkerConfig {
    let worker_name_upper = worker_name.to_uppercase().replace('-', "_");

    WorkerConfig {
        task_definition_name: defaults.task_definition_name,

        poll_interval: resolve_duration_millis(
            &worker_name_upper,
            "POLL_INTERVAL_MILLIS",
            defaults.poll_interval.as_millis() as u64,
        ),

        domain: resolve_string_option(&worker_name_upper, "DOMAIN", defaults.domain),

        worker_id: resolve_string(&worker_name_upper, "WORKER_ID", defaults.worker_id),

        thread_count: resolve_usize(&worker_name_upper, "THREAD_COUNT", defaults.thread_count),

        register_task_def: resolve_bool(
            &worker_name_upper,
            "REGISTER_TASK_DEF",
            defaults.register_task_def,
        ),

        overwrite_task_def: resolve_bool(
            &worker_name_upper,
            "OVERWRITE_TASK_DEF",
            defaults.overwrite_task_def,
        ),

        strict_schema: resolve_bool(&worker_name_upper, "STRICT_SCHEMA", defaults.strict_schema),

        poll_timeout: resolve_duration_millis(
            &worker_name_upper,
            "POLL_TIMEOUT",
            defaults.poll_timeout.as_millis() as u64,
        ),

        paused: resolve_bool(&worker_name_upper, "PAUSED", defaults.paused),
    }
}

/// Generate a unique worker ID
fn generate_worker_id() -> String {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());
    let pid = std::process::id();
    format!("{}-{}", hostname, pid)
}

/// Resolve a string value from environment
fn resolve_string(worker_name: &str, property: &str, default: String) -> String {
    // Try worker-specific first
    if let Ok(val) = env::var(format!("CONDUCTOR_WORKER_{}_{}", worker_name, property)) {
        return val;
    }

    // Try global
    if let Ok(val) = env::var(format!("CONDUCTOR_WORKER_ALL_{}", property)) {
        return val;
    }

    // Also support dot notation (conductor.worker.all.property)
    let dot_global = format!("conductor.worker.all.{}", property.to_lowercase());
    if let Ok(val) = env::var(&dot_global) {
        return val;
    }

    let dot_specific = format!(
        "conductor.worker.{}.{}",
        worker_name.to_lowercase(),
        property.to_lowercase()
    );
    if let Ok(val) = env::var(&dot_specific) {
        return val;
    }

    default
}

/// Resolve an optional string value
fn resolve_string_option(
    worker_name: &str,
    property: &str,
    default: Option<String>,
) -> Option<String> {
    // Try worker-specific first
    if let Ok(val) = env::var(format!("CONDUCTOR_WORKER_{}_{}", worker_name, property)) {
        return Some(val);
    }

    // Try global
    if let Ok(val) = env::var(format!("CONDUCTOR_WORKER_ALL_{}", property)) {
        return Some(val);
    }

    // Also support dot notation
    let dot_global = format!("conductor.worker.all.{}", property.to_lowercase());
    if let Ok(val) = env::var(&dot_global) {
        return Some(val);
    }

    let dot_specific = format!(
        "conductor.worker.{}.{}",
        worker_name.to_lowercase(),
        property.to_lowercase()
    );
    if let Ok(val) = env::var(&dot_specific) {
        return Some(val);
    }

    default
}

/// Resolve a usize value from environment
fn resolve_usize(worker_name: &str, property: &str, default: usize) -> usize {
    let value = resolve_string(worker_name, property, default.to_string());
    value.parse().unwrap_or(default)
}

/// Resolve a boolean value from environment
fn resolve_bool(worker_name: &str, property: &str, default: bool) -> bool {
    let value = resolve_string(worker_name, property, default.to_string());
    matches!(value.to_lowercase().as_str(), "true" | "1" | "yes")
}

/// Resolve a duration in milliseconds
fn resolve_duration_millis(worker_name: &str, property: &str, default_millis: u64) -> Duration {
    let millis = resolve_usize(worker_name, property, default_millis as usize);
    Duration::from_millis(millis as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WorkerConfig::default();
        assert_eq!(config.thread_count, 1);
        assert_eq!(config.poll_interval.as_millis(), 100);
        assert!(!config.paused);
    }

    #[test]
    fn test_worker_config_builder() {
        let config = WorkerConfig::new("test_task")
            .with_thread_count(5)
            .with_poll_interval_millis(500)
            .with_domain("production");

        assert_eq!(config.task_definition_name, "test_task");
        assert_eq!(config.thread_count, 5);
        assert_eq!(config.poll_interval.as_millis(), 500);
        assert_eq!(config.domain, Some("production".to_string()));
    }

    #[test]
    fn test_generate_worker_id() {
        let id = generate_worker_id();
        assert!(!id.is_empty());
        assert!(id.contains('-'));
    }
}
