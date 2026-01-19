//! Task runner events for observability
//!
//! These events are published during task polling and execution,
//! enabling metrics collection and custom monitoring.

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
    pub error: String,
    pub timestamp: DateTime<Utc>,
}

impl PollFailure {
    pub fn new(
        task_type: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        error: impl Into<String>,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            worker_id: worker_id.into(),
            duration,
            error: error.into(),
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
    pub error: String,
    pub is_retryable: bool,
    pub timestamp: DateTime<Utc>,
}

impl TaskExecutionFailure {
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        duration: Duration,
        error: impl Into<String>,
        is_retryable: bool,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            duration,
            error: error.into(),
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

/// Event published when task update fails after all retries
#[derive(Debug, Clone)]
pub struct TaskUpdateFailure {
    pub task_type: String,
    pub task_id: String,
    pub workflow_instance_id: String,
    pub worker_id: String,
    pub error: String,
    pub retry_count: u32,
    pub timestamp: DateTime<Utc>,
}

impl TaskUpdateFailure {
    pub fn new(
        task_type: impl Into<String>,
        task_id: impl Into<String>,
        workflow_instance_id: impl Into<String>,
        worker_id: impl Into<String>,
        error: impl Into<String>,
        retry_count: u32,
    ) -> Self {
        Self {
            task_type: task_type.into(),
            task_id: task_id.into(),
            workflow_instance_id: workflow_instance_id.into(),
            worker_id: worker_id.into(),
            error: error.into(),
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

    /// Called when task execution begins
    fn on_task_execution_started(&self, _event: &TaskExecutionStarted) {}

    /// Called when task execution completes successfully
    fn on_task_execution_completed(&self, _event: &TaskExecutionCompleted) {}

    /// Called when task execution fails
    fn on_task_execution_failure(&self, _event: &TaskExecutionFailure) {}

    /// Called when task update fails after all retries
    fn on_task_update_failure(&self, _event: &TaskUpdateFailure) {}
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
