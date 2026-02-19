// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task definition timeout policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimeoutPolicy {
    /// Retry the task
    Retry,
    /// Timeout the workflow
    #[default]
    TimeOutWf,
    /// Alert only
    AlertOnly,
}

/// Task retry logic
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetryLogic {
    /// Fixed delay between retries
    #[default]
    Fixed,
    /// Exponential backoff
    ExponentialBackoff,
    /// Linear backoff
    LinearBackoff,
}

/// Task definition describing a task type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDef {
    /// Task name (unique identifier)
    pub name: String,

    /// Task description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Number of retry attempts
    #[serde(default)]
    pub retry_count: i32,

    /// Retry logic
    #[serde(default)]
    pub retry_logic: RetryLogic,

    /// Delay between retries in seconds
    #[serde(default)]
    pub retry_delay_seconds: i32,

    /// Backoff scale factor for exponential backoff
    #[serde(default = "default_backoff_factor")]
    pub backoff_scale_factor: i32,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i64,

    /// Response timeout in seconds
    #[serde(default = "default_response_timeout")]
    pub response_timeout_seconds: i64,

    /// Poll timeout in seconds
    #[serde(default)]
    pub poll_timeout_seconds: i64,

    /// Timeout policy
    #[serde(default)]
    pub timeout_policy: TimeoutPolicy,

    /// Maximum concurrent executions (0 = unlimited)
    #[serde(default)]
    pub concurrent_exec_limit: i32,

    /// Rate limit per frequency
    #[serde(default)]
    pub rate_limit_per_frequency: i32,

    /// Rate limit frequency in seconds
    #[serde(default = "default_rate_limit_frequency")]
    pub rate_limit_frequency_in_seconds: i32,

    /// Owner email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_email: Option<String>,

    /// Input keys (expected input parameters)
    #[serde(default)]
    pub input_keys: Vec<String>,

    /// Output keys (expected output parameters)
    #[serde(default)]
    pub output_keys: Vec<String>,

    /// Input template (default input values)
    #[serde(default)]
    pub input_template: HashMap<String, serde_json::Value>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Created time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Updated by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,

    /// Update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Input schema (JSON Schema for input validation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,

    /// Output schema (JSON Schema for output validation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

fn default_backoff_factor() -> i32 {
    1
}

fn default_timeout() -> i64 {
    3600
}

fn default_response_timeout() -> i64 {
    600
}

fn default_rate_limit_frequency() -> i32 {
    1
}

impl Default for TaskDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            retry_count: 0,
            retry_logic: RetryLogic::default(),
            retry_delay_seconds: 0,
            backoff_scale_factor: default_backoff_factor(),
            timeout_seconds: default_timeout(),
            response_timeout_seconds: default_response_timeout(),
            poll_timeout_seconds: 0,
            timeout_policy: TimeoutPolicy::default(),
            concurrent_exec_limit: 0,
            rate_limit_per_frequency: 0,
            rate_limit_frequency_in_seconds: default_rate_limit_frequency(),
            owner_email: None,
            input_keys: Vec::new(),
            output_keys: Vec::new(),
            input_template: HashMap::new(),
            created_by: None,
            create_time: None,
            updated_by: None,
            update_time: None,
            input_schema: None,
            output_schema: None,
        }
    }
}

impl TaskDef {
    /// Create a new task definition with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set retry configuration
    pub fn with_retry(mut self, count: i32, logic: RetryLogic, delay_seconds: i32) -> Self {
        self.retry_count = count;
        self.retry_logic = logic;
        self.retry_delay_seconds = delay_seconds;
        self
    }

    /// Set timeout configuration
    ///
    /// Note: This also sets response_timeout_seconds to the same value to ensure
    /// response_timeout_seconds <= timeout_seconds (required by Conductor validation)
    pub fn with_timeout(mut self, timeout_seconds: i64, policy: TimeoutPolicy) -> Self {
        self.timeout_seconds = timeout_seconds;
        self.timeout_policy = policy;
        // response_timeout must be <= timeout_seconds
        if self.response_timeout_seconds > timeout_seconds {
            self.response_timeout_seconds = timeout_seconds;
        }
        self
    }

    /// Set response timeout
    pub fn with_response_timeout(mut self, seconds: i64) -> Self {
        self.response_timeout_seconds = seconds;
        self
    }

    /// Set rate limit
    pub fn with_rate_limit(mut self, limit: i32, frequency_seconds: i32) -> Self {
        self.rate_limit_per_frequency = limit;
        self.rate_limit_frequency_in_seconds = frequency_seconds;
        self
    }

    /// Set concurrent execution limit
    pub fn with_concurrent_limit(mut self, limit: i32) -> Self {
        self.concurrent_exec_limit = limit;
        self
    }

    /// Set owner email
    pub fn with_owner(mut self, email: impl Into<String>) -> Self {
        self.owner_email = Some(email.into());
        self
    }

    /// Set input keys
    pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
        self.input_keys = keys;
        self
    }

    /// Set output keys
    pub fn with_output_keys(mut self, keys: Vec<String>) -> Self {
        self.output_keys = keys;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_def_builder() {
        let task_def = TaskDef::new("my_task")
            .with_description("A test task")
            .with_retry(3, RetryLogic::LinearBackoff, 5)
            .with_timeout(120, TimeoutPolicy::Retry)
            .with_rate_limit(100, 10);

        assert_eq!(task_def.name, "my_task");
        assert_eq!(task_def.description, Some("A test task".to_string()));
        assert_eq!(task_def.retry_count, 3);
        assert_eq!(task_def.retry_logic, RetryLogic::LinearBackoff);
        assert_eq!(task_def.retry_delay_seconds, 5);
        assert_eq!(task_def.timeout_seconds, 120);
        assert_eq!(task_def.timeout_policy, TimeoutPolicy::Retry);
        assert_eq!(task_def.rate_limit_per_frequency, 100);
        assert_eq!(task_def.rate_limit_frequency_in_seconds, 10);
    }

    #[test]
    fn test_task_def_serialization() {
        let task_def = TaskDef::new("test");
        let json = serde_json::to_string(&task_def).unwrap();
        assert!(json.contains("\"name\":\"test\""));
    }
}
