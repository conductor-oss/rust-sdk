//! Task result model for reporting task execution results

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Status of a task result
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskResultStatus {
    /// Task completed successfully
    #[default]
    Completed,
    /// Task failed (may be retried)
    Failed,
    /// Task failed with terminal error (no retry)
    FailedWithTerminalError,
    /// Task is still in progress
    InProgress,
}

/// Result of a task execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    /// Task ID
    pub task_id: String,

    /// Workflow instance ID
    pub workflow_instance_id: String,

    /// Worker ID that executed the task
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,

    /// Task execution status
    pub status: TaskResultStatus,

    /// Output data from the task
    #[serde(default)]
    pub output_data: HashMap<String, serde_json::Value>,

    /// Reason for failure (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_for_incompletion: Option<String>,

    /// Callback after seconds (for IN_PROGRESS tasks)
    #[serde(default)]
    pub callback_after_seconds: i64,

    /// Logs from task execution
    #[serde(default)]
    pub logs: Vec<TaskExecLog>,

    /// External output payload storage path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_output_payload_storage_path: Option<String>,

    /// Subworkflow ID (for subworkflow tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_workflow_id: Option<String>,

    /// Extend lease (keep-alive for long-running tasks)
    #[serde(default)]
    pub extend_lease: bool,
}

/// Task execution log entry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecLog {
    /// Log message
    pub log: String,

    /// Task ID
    #[serde(default)]
    pub task_id: String,

    /// Created time
    #[serde(default)]
    pub created_time: i64,
}

impl TaskResult {
    /// Create a new completed task result
    pub fn completed(task_id: impl Into<String>, workflow_instance_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            status: TaskResultStatus::Completed,
            ..Default::default()
        }
    }

    /// Create a new failed task result
    pub fn failed(
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            status: TaskResultStatus::Failed,
            reason_for_incompletion: Some(reason.into()),
            ..Default::default()
        }
    }

    /// Create an in-progress task result (for long-running tasks)
    pub fn in_progress(
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        callback_after_seconds: i64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            status: TaskResultStatus::InProgress,
            callback_after_seconds,
            ..Default::default()
        }
    }

    /// Set worker ID
    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    /// Set output data
    pub fn with_output(mut self, output: HashMap<String, serde_json::Value>) -> Self {
        self.output_data = output;
        self
    }

    /// Set a single output value
    pub fn with_output_value(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.output_data.insert(
            key.into(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// Add a log entry
    pub fn with_log(mut self, log: impl Into<String>) -> Self {
        self.logs.push(TaskExecLog {
            log: log.into(),
            task_id: self.task_id.clone(),
            created_time: chrono::Utc::now().timestamp_millis(),
        });
        self
    }

    /// Set failure reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason_for_incompletion = Some(reason.into());
        self
    }

    /// Extend the lease (for long-running tasks)
    pub fn with_extend_lease(mut self, extend: bool) -> Self {
        self.extend_lease = extend;
        self
    }
}

/// Task in progress marker for long-running tasks
///
/// Return this from a worker to indicate the task is still processing
/// and should be polled again after `callback_after_seconds`.
#[derive(Debug, Clone)]
pub struct TaskInProgress {
    /// Seconds until the task should be polled again
    pub callback_after_seconds: i64,

    /// Intermediate output data
    pub output: HashMap<String, serde_json::Value>,
}

impl TaskInProgress {
    /// Create a new task in progress marker
    pub fn new(callback_after_seconds: i64) -> Self {
        Self {
            callback_after_seconds,
            output: HashMap::new(),
        }
    }

    /// Add intermediate output data
    pub fn with_output(mut self, output: HashMap<String, serde_json::Value>) -> Self {
        self.output = output;
        self
    }

    /// Add a single output value
    pub fn with_output_value(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.output.insert(
            key.into(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }
}

impl Default for TaskInProgress {
    fn default() -> Self {
        Self {
            callback_after_seconds: 60,
            output: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completed_result() {
        let result = TaskResult::completed("task-1", "wf-1")
            .with_worker_id("worker-1")
            .with_output_value("result", "success");

        assert_eq!(result.task_id, "task-1");
        assert_eq!(result.status, TaskResultStatus::Completed);
        assert_eq!(result.worker_id, Some("worker-1".to_string()));
        assert!(result.output_data.contains_key("result"));
    }

    #[test]
    fn test_failed_result() {
        let result = TaskResult::failed("task-1", "wf-1", "Something went wrong");

        assert_eq!(result.status, TaskResultStatus::Failed);
        assert_eq!(
            result.reason_for_incompletion,
            Some("Something went wrong".to_string())
        );
    }

    #[test]
    fn test_in_progress_result() {
        let result = TaskResult::in_progress("task-1", "wf-1", 30);

        assert_eq!(result.status, TaskResultStatus::InProgress);
        assert_eq!(result.callback_after_seconds, 30);
    }

    #[test]
    fn test_task_in_progress() {
        let tip = TaskInProgress::new(60).with_output_value("progress", 50);

        assert_eq!(tip.callback_after_seconds, 60);
        assert!(tip.output.contains_key("progress"));
    }
}
