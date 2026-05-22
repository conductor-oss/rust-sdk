// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Base trait for all task runner events
pub trait TaskRunnerEvent: Send + Sync {
    /// Get the task type
    fn task_type(&self) -> &str;

    /// Get the timestamp when the event occurred
    fn timestamp(&self) -> DateTime<Utc>;
}

/// Event published when polling starts for a task type
#[derive(Debug, Clone)]
pub struct PollStarted {
    pub task_type: String,
    pub worker_id: String,
    pub poll_count: usize,
    pub timestamp: DateTime<Utc>,
}

impl PollStarted {
    pub fn new(
        task_type: impl Into<String>,
        worker_id: impl Into<String>,
        poll_count: usize,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            worker_id: worker_id.into(),
            poll_count,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for PollStarted {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when polling completes successfully
#[derive(Debug, Clone)]
pub struct PollCompleted {
    pub task_type: String,
    pub worker_id: String,
    pub duration: Duration,
    pub tasks_received: usize,
    pub timestamp: DateTime<Utc>,
}

impl PollCompleted {
    pub fn new(
        task_type: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        tasks_received: usize,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            worker_id: worker_id.into(),
            duration,
            tasks_received,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for PollCompleted {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when polling fails
#[derive(Debug, Clone)]
pub struct PollFailure {
    pub task_type: String,
    pub worker_id: String,
    pub duration: Duration,
    /// Human-readable error message (from `Display`). Not a metric label.
    pub error: String,
    /// Canonical exception *type name* for the `exception` metric label.
    pub exception: String,
    pub timestamp: DateTime<Utc>,
}

impl PollFailure {
    pub fn new(
        task_type: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        error: impl Into<String>,
        exception: impl Into<String>,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            worker_id: worker_id.into(),
            duration,
            error: error.into(),
            exception: exception.into(),
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for PollFailure {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when the runner skips a poll because the worker is paused.
#[derive(Debug, Clone)]
pub struct PollSkippedPaused {
    pub task_type: String,
    pub worker_id: String,
    pub timestamp: DateTime<Utc>,
}

impl PollSkippedPaused {
    pub fn new(task_type: impl Into<String>, worker_id: impl Into<String>) -> Self {
        Self {
            task_type: task_type.into(),
            worker_id: worker_id.into(),
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for PollSkippedPaused {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when task execution starts
#[derive(Debug, Clone)]
pub struct TaskExecutionStarted {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    pub timestamp: DateTime<Utc>,
}

impl TaskExecutionStarted {
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for TaskExecutionStarted {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when task execution completes successfully
#[derive(Debug, Clone)]
pub struct TaskExecutionCompleted {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    pub duration: Duration,
    pub output_size_bytes: Option<usize>,
    pub timestamp: DateTime<Utc>,
}

impl TaskExecutionCompleted {
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        output_size_bytes: Option<usize>,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            duration,
            output_size_bytes,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for TaskExecutionCompleted {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when task execution fails
#[derive(Debug, Clone)]
pub struct TaskExecutionFailure {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    pub duration: Duration,
    /// Human-readable error message. Not a metric label.
    pub error: String,
    /// Canonical exception *type name* for the `exception` metric label.
    pub exception: String,
    pub is_retryable: bool,
    pub timestamp: DateTime<Utc>,
}

impl TaskExecutionFailure {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        error: impl Into<String>,
        exception: impl Into<String>,
        is_retryable: bool,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            duration,
            error: error.into(),
            exception: exception.into(),
            is_retryable,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for TaskExecutionFailure {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when a `TaskClient::update_task*` call succeeds.
///
/// Fires once per completed task; used to observe the
/// `task_update_time_seconds{status="SUCCESS"}` histogram.
#[derive(Debug, Clone)]
pub struct TaskUpdateCompleted {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    pub duration: Duration,
    pub timestamp: DateTime<Utc>,
}

impl TaskUpdateCompleted {
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            duration,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for TaskUpdateCompleted {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when task update fails after all retries
#[derive(Debug, Clone)]
pub struct TaskUpdateFailure {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    /// Wall-clock duration of the (final, failed) update attempt sequence.
    pub duration: Duration,
    /// Human-readable error message. Not a metric label.
    pub error: String,
    /// Canonical exception *type name* for the `exception` metric label.
    pub exception: String,
    pub retry_count: u32,
    pub timestamp: DateTime<Utc>,
}

impl TaskUpdateFailure {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        error: impl Into<String>,
        exception: impl Into<String>,
        retry_count: u32,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            duration,
            error: error.into(),
            exception: exception.into(),
            retry_count,
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for TaskUpdateFailure {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published when a spawned worker task terminates with an uncaught
/// panic. Used to populate `thread_uncaught_exceptions_total`.
#[derive(Debug, Clone)]
pub struct ThreadUncaughtException {
    pub task_type: String,
    /// Canonical exception *type name* for the `exception` metric label.
    pub exception: String,
    pub timestamp: DateTime<Utc>,
}

impl ThreadUncaughtException {
    pub fn new(task_type: impl Into<String>, exception: impl Into<String>) -> Self {
        Self {
            task_type: task_type.into(),
            exception: exception.into(),
            timestamp: Utc::now(),
        }
    }
}

impl TaskRunnerEvent for ThreadUncaughtException {
    fn task_type(&self) -> &str {
        &self.task_type
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Event published by [`WorkflowClient::start_workflow`](crate::client::WorkflowClient::start_workflow)
/// after a successful call. Carries the serialized input byte size so the
/// `workflow_input_size_bytes` gauge can be updated.
#[derive(Debug, Clone)]
pub struct WorkflowStarted {
    pub workflow_type: String,
    pub version: Option<i32>,
    pub input_size_bytes: usize,
    pub timestamp: DateTime<Utc>,
}

impl WorkflowStarted {
    pub fn new(
        workflow_type: impl Into<String>,
        version: Option<i32>,
        input_size_bytes: usize,
    ) -> Self {
        Self {
            workflow_type: workflow_type.into(),
            version,
            input_size_bytes,
            timestamp: Utc::now(),
        }
    }
}

/// Event published by [`WorkflowClient::start_workflow`](crate::client::WorkflowClient::start_workflow)
/// when the HTTP call returns an error. Used to populate
/// `workflow_start_error_total{workflowType, exception}`.
#[derive(Debug, Clone)]
pub struct WorkflowStartFailure {
    pub workflow_type: String,
    /// Canonical exception *type name* for the `exception` metric label.
    pub exception: String,
    pub timestamp: DateTime<Utc>,
}

impl WorkflowStartFailure {
    pub fn new(workflow_type: impl Into<String>, exception: impl Into<String>) -> Self {
        Self {
            workflow_type: workflow_type.into(),
            exception: exception.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Listener trait for task runner events
///
/// Implement this trait to receive task execution lifecycle events.
/// All methods have default empty implementations, so you only need
/// to implement the events you care about.
pub trait TaskRunnerEventsListener: Send + Sync {
    /// Called when polling starts for a task type
    fn on_poll_started(&self, _event: &PollStarted) {}

    /// Called when polling completes successfully
    fn on_poll_completed(&self, _event: &PollCompleted) {}

    /// Called when polling fails
    fn on_poll_failure(&self, _event: &PollFailure) {}

    /// Called when a poll is skipped because the worker is paused
    fn on_poll_skipped_paused(&self, _event: &PollSkippedPaused) {}

    /// Called when task execution begins
    fn on_task_execution_started(&self, _event: &TaskExecutionStarted) {}

    /// Called when task execution completes successfully
    fn on_task_execution_completed(&self, _event: &TaskExecutionCompleted) {}

    /// Called when task execution fails
    fn on_task_execution_failure(&self, _event: &TaskExecutionFailure) {}

    /// Called when a task update to the server completes successfully
    fn on_task_update_completed(&self, _event: &TaskUpdateCompleted) {}

    /// Called when task update fails after all retries
    fn on_task_update_failure(&self, _event: &TaskUpdateFailure) {}

    /// Called when a spawned worker task terminates via uncaught panic
    fn on_thread_uncaught_exception(&self, _event: &ThreadUncaughtException) {}

    /// Called after a workflow is successfully started via `WorkflowClient`
    fn on_workflow_started(&self, _event: &WorkflowStarted) {}

    /// Called when a `WorkflowClient::start_workflow` call fails
    fn on_workflow_start_failure(&self, _event: &WorkflowStartFailure) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct TestListener {
        poll_started_count: AtomicUsize,
        execution_completed_count: AtomicUsize,
    }

    impl TestListener {
        fn new() -> Self {
            Self {
                poll_started_count: AtomicUsize::new(0),
                execution_completed_count: AtomicUsize::new(0),
            }
        }
    }

    impl TaskRunnerEventsListener for TestListener {
        fn on_poll_started(&self, _event: &PollStarted) {
            self.poll_started_count.fetch_add(1, Ordering::SeqCst);
        }

        fn on_task_execution_completed(&self, _event: &TaskExecutionCompleted) {
            self.execution_completed_count
                .fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_poll_started_event() {
        let event = PollStarted::new("test_task", "worker-1", 10);
        assert_eq!(event.task_type(), "test_task");
        assert_eq!(event.poll_count, 10);
    }

    #[test]
    fn test_listener() {
        let listener = Arc::new(TestListener::new());

        let event = PollStarted::new("test_task", "worker-1", 10);
        listener.on_poll_started(&event);
        assert_eq!(listener.poll_started_count.load(Ordering::SeqCst), 1);

        let event = TaskExecutionCompleted::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
            Duration::from_millis(100),
            Some(1024),
        );
        listener.on_task_execution_completed(&event);
        assert_eq!(listener.execution_completed_count.load(Ordering::SeqCst), 1);
    }
}
