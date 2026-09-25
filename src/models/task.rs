// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Module for flexible timestamp deserialization (handles both i64 and ISO date strings).
mod timestamp_deserializer {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer};

    /// Deserialize a timestamp that may be either i64 (epoch ms) or ISO 8601 string.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TimestampOrString {
            Timestamp(i64),
            String(String),
        }

        match TimestampOrString::deserialize(deserializer)? {
            TimestampOrString::Timestamp(ts) => Ok(ts),
            TimestampOrString::String(s) => {
                // Try to parse as ISO 8601 date string
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
                    .or_else(|_| {
                        // Try alternative formats
                        s.parse::<i64>()
                    })
                    .map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Status of a task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Task is scheduled but not yet picked up.
    #[default]
    Scheduled,
    /// Task is currently being executed.
    InProgress,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task failed with terminal error (no retry).
    FailedWithTerminalError,
    /// Task was canceled.
    Canceled,
    /// Task was skipped.
    Skipped,
    /// Task timed out.
    TimedOut,
}

/// A task in a Conductor workflow.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique task ID.
    #[serde(default)]
    pub task_id: String,

    /// Task type/definition name.
    #[serde(default)]
    pub task_type: String,

    /// Reference name within the workflow.
    #[serde(default)]
    pub reference_task_name: String,

    /// Task status.
    #[serde(default)]
    pub status: TaskStatus,

    /// Input data for the task.
    #[serde(default)]
    pub input_data: HashMap<String, serde_json::Value>,

    /// Output data from the task.
    #[serde(default)]
    pub output_data: HashMap<String, serde_json::Value>,

    /// Workflow instance ID this task belongs to.
    #[serde(default)]
    pub workflow_instance_id: String,

    /// Workflow type/name.
    #[serde(default)]
    pub workflow_type: String,

    /// Task definition name.
    #[serde(default)]
    pub task_def_name: String,

    /// Current retry count.
    #[serde(default)]
    pub retry_count: i32,

    /// Poll count (for long-running tasks).
    #[serde(default)]
    pub poll_count: i32,

    /// Worker that picked up this task.
    #[serde(default)]
    pub worker_id: Option<String>,

    /// Domain the task is running in.
    #[serde(default)]
    pub domain: Option<String>,

    /// Scheduled time (epoch ms or ISO 8601 string).
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub scheduled_time: i64,

    /// Start time (epoch ms or ISO 8601 string).
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub start_time: i64,

    /// End time (epoch ms or ISO 8601 string).
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub end_time: i64,

    /// Update time (epoch ms or ISO 8601 string).
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub update_time: i64,

    /// Queue wait time (ms).
    #[serde(default)]
    pub queue_wait_time: i64,

    /// Callback after seconds (for `IN_PROGRESS` tasks).
    #[serde(default)]
    pub callback_after_seconds: i64,

    /// Response timeout seconds.
    #[serde(default)]
    pub response_timeout_seconds: i64,

    /// Execution namespace ID.
    #[serde(default)]
    pub execution_name_space: Option<String>,

    /// Isolation group ID.
    #[serde(default)]
    pub isolation_group_id: Option<String>,

    /// Correlation ID.
    #[serde(default)]
    pub correlation_id: Option<String>,

    /// Reason for failure (if failed).
    #[serde(default)]
    pub reason_for_incompletion: Option<String>,

    /// External input payload storage path.
    #[serde(default)]
    pub external_input_payload_storage_path: Option<String>,

    /// External output payload storage path.
    #[serde(default)]
    pub external_output_payload_storage_path: Option<String>,

    /// Task execution logs.
    #[serde(default)]
    pub logs: Vec<TaskExecLog>,

    /// Subworkflow ID (if this is a subworkflow task).
    #[serde(default)]
    pub sub_workflow_id: Option<String>,

    /// Iteration count (for loop tasks).
    #[serde(default)]
    pub iteration: i32,

    /// Server-resolved credential values for this poll, keyed by declared name. Read via
    /// [`crate::agents::Credentials::from_task`] rather than directly. May contain secrets --
    /// see the manual [`std::fmt::Debug`] impl below, which redacts values.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub runtime_metadata: HashMap<String, String>,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut runtime_metadata_keys: Vec<&str> =
            self.runtime_metadata.keys().map(String::as_str).collect();
        runtime_metadata_keys.sort_unstable();
        f.debug_struct("Task")
            .field("task_id", &self.task_id)
            .field("task_type", &self.task_type)
            .field("reference_task_name", &self.reference_task_name)
            .field("status", &self.status)
            .field("input_data", &self.input_data)
            .field("output_data", &self.output_data)
            .field("workflow_instance_id", &self.workflow_instance_id)
            .field("workflow_type", &self.workflow_type)
            .field("task_def_name", &self.task_def_name)
            .field("retry_count", &self.retry_count)
            .field("poll_count", &self.poll_count)
            .field("worker_id", &self.worker_id)
            .field("domain", &self.domain)
            .field("scheduled_time", &self.scheduled_time)
            .field("start_time", &self.start_time)
            .field("end_time", &self.end_time)
            .field("update_time", &self.update_time)
            .field("queue_wait_time", &self.queue_wait_time)
            .field("callback_after_seconds", &self.callback_after_seconds)
            .field("response_timeout_seconds", &self.response_timeout_seconds)
            .field("execution_name_space", &self.execution_name_space)
            .field("isolation_group_id", &self.isolation_group_id)
            .field("correlation_id", &self.correlation_id)
            .field("reason_for_incompletion", &self.reason_for_incompletion)
            .field(
                "external_input_payload_storage_path",
                &self.external_input_payload_storage_path,
            )
            .field(
                "external_output_payload_storage_path",
                &self.external_output_payload_storage_path,
            )
            .field("logs", &self.logs)
            .field("sub_workflow_id", &self.sub_workflow_id)
            .field("iteration", &self.iteration)
            // Names only -- never the resolved values, which may be secrets.
            .field("runtime_metadata_keys", &runtime_metadata_keys)
            .finish()
    }
}

impl Task {
    /// Get a typed input value.
    #[must_use]
    pub fn get_input<T>(&self, key: &str) -> Option<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let v = self.input_data.get(key)?;
        serde_json::from_value(v.clone()).ok()
    }

    /// Get input as string.
    #[must_use]
    pub fn get_input_string(&self, key: &str) -> Option<String> {
        self.input_data.get(key).map(|v| {
            if let serde_json::Value::String(s) = v {
                s.clone()
            } else {
                v.to_string()
            }
        })
    }

    /// Check if task is terminal (completed, failed, etc.)
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::FailedWithTerminalError
                | TaskStatus::Canceled
                | TaskStatus::Skipped
                | TaskStatus::TimedOut
        )
    }

    /// Get task context for accessing task metadata.
    ///
    /// Provides convenient access to task ID, workflow ID, poll count, etc.
    ///
    /// # Example
    ///
    /// ```rust
    /// use conductor::models::Task;
    ///
    /// fn process_task(task: &Task) {
    ///     let ctx = task.context();
    ///     
    ///     println!("Task ID: {}", ctx.task_id());
    ///     println!("Workflow ID: {}", ctx.workflow_instance_id());
    ///     println!("Poll count: {}", ctx.poll_count());
    ///     
    ///     if ctx.is_first_poll() {
    ///         println!("This is the first poll");
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn context(&self) -> crate::worker::TaskContext {
        crate::worker::TaskContext::from_task(self)
    }

    /// Get the task ID.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the workflow instance ID.
    #[must_use]
    pub fn workflow_instance_id(&self) -> &str {
        &self.workflow_instance_id
    }

    /// Get the poll count.
    #[must_use]
    pub fn poll_count(&self) -> i32 {
        self.poll_count
    }

    /// Get the retry count.
    #[must_use]
    pub fn retry_count(&self) -> i32 {
        self.retry_count
    }

    /// Check if this is the first poll (`poll_count` == 0).
    #[must_use]
    pub fn is_first_poll(&self) -> bool {
        self.poll_count == 0
    }

    /// Check if this is a retry (`retry_count` > 0).
    #[must_use]
    pub fn is_retry(&self) -> bool {
        self.retry_count > 0
    }
}

/// Task execution log entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecLog {
    /// Log message.
    pub log: String,

    /// Task ID.
    pub task_id: String,

    /// Created time (epoch ms).
    pub created_time: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_deserialization() {
        let json = r#"{
            "taskId": "task-123",
            "taskType": "simple_task",
            "status": "IN_PROGRESS",
            "inputData": {"name": "test"},
            "workflowInstanceId": "wf-456"
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.task_id, "task-123");
        assert_eq!(task.task_type, "simple_task");
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.get_input_string("name"), Some("test".to_owned()));
    }

    #[test]
    fn test_task_debug_redacts_runtime_metadata_values() {
        let task = Task {
            runtime_metadata: HashMap::from([(
                "GH_TOKEN".to_owned(),
                "ghp_super_secret".to_owned(),
            )]),
            ..Default::default()
        };

        let debug = format!("{task:?}");
        assert!(debug.contains("GH_TOKEN"));
        assert!(!debug.contains("ghp_super_secret"));
    }

    #[test]
    fn test_task_is_terminal() {
        let task = Task {
            status: TaskStatus::InProgress,
            ..Default::default()
        };
        assert!(!task.is_terminal());

        let task = Task {
            status: TaskStatus::Completed,
            ..Default::default()
        };
        assert!(task.is_terminal());

        let task = Task {
            status: TaskStatus::Failed,
            ..Default::default()
        };
        assert!(task.is_terminal());
    }
}
