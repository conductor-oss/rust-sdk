// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::models::Task;

/// Context for the currently executing task.
///
/// This provides convenient access to task metadata and execution state.
///
/// # Example
///
/// ```rust,no_run
/// use conductor::worker::{Worker, WorkerOutput, TaskContext};
/// use conductor::models::Task;
///
/// async fn process_long_task(task: &Task) -> WorkerOutput {
///     let ctx = TaskContext::from_task(task);
///     
///     // Check how many times this task has been polled
///     let poll_count = ctx.poll_count();
///     
///     if poll_count > 10 {
///         // Too many iterations, fail
///         return WorkerOutput::failed("Max iterations exceeded");
///     }
///     
///     // Process work based on poll_count
///     let processed = process_chunk(poll_count);
///     
///     if is_complete() {
///         WorkerOutput::completed_with_result(processed)
///     } else {
///         // More work to do
///         WorkerOutput::in_progress(30)
///     }
/// }
/// # fn process_chunk(_: i32) -> String { String::new() }
/// # fn is_complete() -> bool { true }
/// ```
#[derive(Debug, Clone)]
pub struct TaskContext {
    task_id: String,
    workflow_instance_id: String,
    task_type: String,
    reference_task_name: String,
    poll_count: i32,
    retry_count: i32,
    correlation_id: Option<String>,
    domain: Option<String>,
    scheduled_time: i64,
    start_time: i64,
    iteration: i32,
}

impl TaskContext {
    /// Create a `TaskContext` from a Task.
    #[must_use]
    pub fn from_task(task: &Task) -> Self {
        Self {
            task_id: task.task_id.clone(),
            workflow_instance_id: task.workflow_instance_id.clone(),
            task_type: task.task_type.clone(),
            reference_task_name: task.reference_task_name.clone(),
            poll_count: task.poll_count,
            retry_count: task.retry_count,
            correlation_id: task.correlation_id.clone(),
            domain: task.domain.clone(),
            scheduled_time: task.scheduled_time,
            start_time: task.start_time,
            iteration: task.iteration,
        }
    }

    /// Get the unique task ID.
    #[must_use]
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// Get the workflow instance ID.
    #[must_use]
    pub fn workflow_instance_id(&self) -> &str {
        &self.workflow_instance_id
    }

    /// Get the task type (definition name).
    #[must_use]
    pub fn task_type(&self) -> &str {
        &self.task_type
    }

    /// Get the reference task name within the workflow.
    #[must_use]
    pub fn reference_task_name(&self) -> &str {
        &self.reference_task_name
    }

    /// Get the poll count for long-running tasks.
    ///
    /// This increments each time the task is polled after returning
    /// `TaskInProgress`. Use this to track progress or implement
    /// pagination in long-running tasks.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use conductor::worker::TaskContext;
    /// # use conductor::models::Task;
    /// let task = Task::default();
    /// let ctx = TaskContext::from_task(&task);
    ///
    /// // Process different chunks based on poll count
    /// let offset = ctx.poll_count() * 100;
    /// // process_items(offset, 100);
    /// ```
    #[must_use]
    pub fn poll_count(&self) -> i32 {
        self.poll_count
    }

    /// Get the current retry count.
    ///
    /// Indicates how many times this task has been retried after failures.
    #[must_use]
    pub fn retry_count(&self) -> i32 {
        self.retry_count
    }

    /// Get the correlation ID (if set).
    #[must_use]
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    /// Get the domain (if set).
    #[must_use]
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    /// Get the scheduled time (epoch milliseconds).
    #[must_use]
    pub fn scheduled_time(&self) -> i64 {
        self.scheduled_time
    }

    /// Get the start time (epoch milliseconds).
    #[must_use]
    pub fn start_time(&self) -> i64 {
        self.start_time
    }

    /// Get the iteration count (for loop tasks).
    #[must_use]
    pub fn iteration(&self) -> i32 {
        self.iteration
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_context_from_task() {
        let task = Task {
            task_id: "task-123".to_owned(),
            workflow_instance_id: "wf-456".to_owned(),
            task_type: "my_task".to_owned(),
            reference_task_name: "my_task_ref".to_owned(),
            poll_count: 5,
            retry_count: 1,
            correlation_id: Some("corr-789".to_owned()),
            domain: Some("production".to_owned()),
            ..Default::default()
        };

        let ctx = TaskContext::from_task(&task);

        assert_eq!(ctx.task_id(), "task-123");
        assert_eq!(ctx.workflow_instance_id(), "wf-456");
        assert_eq!(ctx.task_type(), "my_task");
        assert_eq!(ctx.reference_task_name(), "my_task_ref");
        assert_eq!(ctx.poll_count(), 5);
        assert_eq!(ctx.retry_count(), 1);
        assert_eq!(ctx.correlation_id(), Some("corr-789"));
        assert_eq!(ctx.domain(), Some("production"));
        assert!(!ctx.is_first_poll());
        assert!(ctx.is_retry());
    }

    #[test]
    fn test_first_poll() {
        let task = Task {
            poll_count: 0,
            retry_count: 0,
            ..Default::default()
        };

        let ctx = TaskContext::from_task(&task);

        assert!(ctx.is_first_poll());
        assert!(!ctx.is_retry());
    }
}
