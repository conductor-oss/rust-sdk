//! Task model representing a unit of work in a workflow

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Module for flexible timestamp deserialization (handles both i64 and ISO date strings)
mod timestamp_deserializer {
    use serde::{self, Deserialize, Deserializer};
    use chrono::{DateTime, Utc};

    /// Deserialize a timestamp that may be either i64 (epoch ms) or ISO 8601 string
    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TimestampOrString {
            Timestamp(i64),
            #[allow(dead_code)]
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

/// Status of a task
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    /// Task is scheduled but not yet picked up
    #[default]
    Scheduled,
    /// Task is currently being executed
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task failed with terminal error (no retry)
    FailedWithTerminalError,
    /// Task was canceled
    Canceled,
    /// Task was skipped
    Skipped,
    /// Task timed out
    TimedOut,
}

/// A task in a Conductor workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique task ID
    #[serde(default)]
    pub task_id: String,

    /// Task type/definition name
    #[serde(default)]
    pub task_type: String,

    /// Reference name within the workflow
    #[serde(default)]
    pub reference_task_name: String,

    /// Task status
    #[serde(default)]
    pub status: TaskStatus,

    /// Input data for the task
    #[serde(default)]
    pub input_data: HashMap<String, serde_json::Value>,

    /// Output data from the task
    #[serde(default)]
    pub output_data: HashMap<String, serde_json::Value>,

    /// Workflow instance ID this task belongs to
    #[serde(default)]
    pub workflow_instance_id: String,

    /// Workflow type/name
    #[serde(default)]
    pub workflow_type: String,

    /// Task definition name
    #[serde(default)]
    pub task_def_name: String,

    /// Current retry count
    #[serde(default)]
    pub retry_count: i32,

    /// Poll count (for long-running tasks)
    #[serde(default)]
    pub poll_count: i32,

    /// Worker that picked up this task
    #[serde(default)]
    pub worker_id: Option<String>,

    /// Domain the task is running in
    #[serde(default)]
    pub domain: Option<String>,

    /// Scheduled time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub scheduled_time: i64,

    /// Start time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub start_time: i64,

    /// End time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub end_time: i64,

    /// Update time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub update_time: i64,

    /// Queue wait time (ms)
    #[serde(default)]
    pub queue_wait_time: i64,

    /// Callback after seconds (for IN_PROGRESS tasks)
    #[serde(default)]
    pub callback_after_seconds: i64,

    /// Response timeout seconds
    #[serde(default)]
    pub response_timeout_seconds: i64,

    /// Execution namespace ID
    #[serde(default)]
    pub execution_name_space: Option<String>,

    /// Isolation group ID
    #[serde(default)]
    pub isolation_group_id: Option<String>,

    /// Correlation ID
    #[serde(default)]
    pub correlation_id: Option<String>,

    /// Reason for failure (if failed)
    #[serde(default)]
    pub reason_for_incompletion: Option<String>,

    /// External input payload storage path
    #[serde(default)]
    pub external_input_payload_storage_path: Option<String>,

    /// External output payload storage path
    #[serde(default)]
    pub external_output_payload_storage_path: Option<String>,

    /// Task execution logs
    #[serde(default)]
    pub logs: Vec<TaskExecLog>,

    /// Subworkflow ID (if this is a subworkflow task)
    #[serde(default)]
    pub sub_workflow_id: Option<String>,

    /// Iteration count (for loop tasks)
    #[serde(default)]
    pub iteration: i32,
}

impl Task {
    /// Get a typed input value
    pub fn get_input<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.input_data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get input as string
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

    /// Get task context for accessing task metadata
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
    pub fn context(&self) -> crate::worker::TaskContext {
        crate::worker::TaskContext::from_task(self)
    }

    /// Get the task ID
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the workflow instance ID
    pub fn workflow_instance_id(&self) -> &str {
        &self.workflow_instance_id
    }

    /// Get the poll count
    pub fn poll_count(&self) -> i32 {
        self.poll_count
    }

    /// Get the retry count
    pub fn retry_count(&self) -> i32 {
        self.retry_count
    }

    /// Check if this is the first poll (poll_count == 0)
    pub fn is_first_poll(&self) -> bool {
        self.poll_count == 0
    }

    /// Check if this is a retry (retry_count > 0)
    pub fn is_retry(&self) -> bool {
        self.retry_count > 0
    }
}

/// Task execution log entry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecLog {
    /// Log message
    pub log: String,

    /// Task ID
    pub task_id: String,

    /// Created time (epoch ms)
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
        assert_eq!(task.get_input_string("name"), Some("test".to_string()));
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
