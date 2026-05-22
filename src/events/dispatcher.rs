// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use parking_lot::RwLock;
use std::sync::Arc;
use tracing::error;

use super::{
    PollCompleted, PollFailure, PollSkippedPaused, PollStarted, TaskExecutionCompleted,
    TaskExecutionFailure, TaskExecutionStarted, TaskRunnerEventsListener, TaskUpdateCompleted,
    TaskUpdateFailure, ThreadUncaughtException, WorkflowStartFailure, WorkflowStarted,
};

/// Async event dispatcher for task runner events
///
/// Thread-safe event dispatcher that allows concurrent event publishing
/// and listener registration. Listeners are cloned before iteration to
/// minimize lock contention.
pub struct EventDispatcher {
    listeners: Arc<RwLock<Vec<Arc<dyn TaskRunnerEventsListener>>>>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventDispatcher {
    fn clone(&self) -> Self {
        Self {
            listeners: Arc::clone(&self.listeners),
        }
    }
}

impl EventDispatcher {
    /// Create a new event dispatcher
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a listener
    pub fn register(&self, listener: Arc<dyn TaskRunnerEventsListener>) {
        self.listeners.write().push(listener);
    }

    /// Unregister all listeners
    pub fn clear(&self) {
        self.listeners.write().clear();
    }

    /// Get the number of registered listeners
    pub fn listener_count(&self) -> usize {
        self.listeners.read().len()
    }

    /// Get a snapshot of current listeners (releases lock immediately)
    #[inline]
    fn get_listeners(&self) -> Vec<Arc<dyn TaskRunnerEventsListener>> {
        self.listeners.read().clone()
    }

    /// Publish a poll started event
    pub fn publish_poll_started(&self, event: &PollStarted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_poll_started(event);
            })) {
                error!("Listener panicked on poll_started: {:?}", e);
            }
        }
    }

    /// Publish a poll completed event
    pub fn publish_poll_completed(&self, event: &PollCompleted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_poll_completed(event);
            })) {
                error!("Listener panicked on poll_completed: {:?}", e);
            }
        }
    }

    /// Publish a poll failure event
    pub fn publish_poll_failure(&self, event: &PollFailure) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_poll_failure(event);
            })) {
                error!("Listener panicked on poll_failure: {:?}", e);
            }
        }
    }

    /// Publish a poll-skipped-due-to-pause event
    pub fn publish_poll_skipped_paused(&self, event: &PollSkippedPaused) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_poll_skipped_paused(event);
            })) {
                error!("Listener panicked on poll_skipped_paused: {:?}", e);
            }
        }
    }

    /// Publish a task execution started event
    pub fn publish_task_execution_started(&self, event: &TaskExecutionStarted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_task_execution_started(event);
            })) {
                error!("Listener panicked on task_execution_started: {:?}", e);
            }
        }
    }

    /// Publish a task execution completed event
    pub fn publish_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_task_execution_completed(event);
            })) {
                error!("Listener panicked on task_execution_completed: {:?}", e);
            }
        }
    }

    /// Publish a task execution failure event
    pub fn publish_task_execution_failure(&self, event: &TaskExecutionFailure) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_task_execution_failure(event);
            })) {
                error!("Listener panicked on task_execution_failure: {:?}", e);
            }
        }
    }

    /// Publish a task update completed event
    pub fn publish_task_update_completed(&self, event: &TaskUpdateCompleted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_task_update_completed(event);
            })) {
                error!("Listener panicked on task_update_completed: {:?}", e);
            }
        }
    }

    /// Publish a task update failure event
    pub fn publish_task_update_failure(&self, event: &TaskUpdateFailure) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_task_update_failure(event);
            })) {
                error!("Listener panicked on task_update_failure: {:?}", e);
            }
        }
    }

    /// Publish an uncaught-panic event
    pub fn publish_thread_uncaught_exception(&self, event: &ThreadUncaughtException) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_thread_uncaught_exception(event);
            })) {
                error!("Listener panicked on thread_uncaught_exception: {:?}", e);
            }
        }
    }

    /// Publish a workflow started event
    pub fn publish_workflow_started(&self, event: &WorkflowStarted) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_workflow_started(event);
            })) {
                error!("Listener panicked on workflow_started: {:?}", e);
            }
        }
    }

    /// Publish a workflow start failure event
    pub fn publish_workflow_start_failure(&self, event: &WorkflowStartFailure) {
        let listeners = self.get_listeners();
        for listener in listeners {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener.on_workflow_start_failure(event);
            })) {
                error!("Listener panicked on workflow_start_failure: {:?}", e);
            }
        }
    }
}

/// Synchronous event dispatcher (for use in non-async contexts)
pub type SyncEventDispatcher = EventDispatcher;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct CountingListener {
        count: AtomicUsize,
    }

    impl CountingListener {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
            }
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }
    }

    impl TaskRunnerEventsListener for CountingListener {
        fn on_poll_started(&self, _event: &PollStarted) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn on_task_execution_completed(&self, _event: &TaskExecutionCompleted) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_dispatcher_registration() {
        let dispatcher = EventDispatcher::new();
        let listener = Arc::new(CountingListener::new());

        assert_eq!(dispatcher.listener_count(), 0);

        dispatcher.register(listener);
        assert_eq!(dispatcher.listener_count(), 1);

        dispatcher.clear();
        assert_eq!(dispatcher.listener_count(), 0);
    }

    #[test]
    fn test_event_publishing() {
        let dispatcher = EventDispatcher::new();
        let listener = Arc::new(CountingListener::new());
        dispatcher.register(listener.clone());

        let event = PollStarted::new("test_task", "worker-1", 10);
        dispatcher.publish_poll_started(&event);
        assert_eq!(listener.count(), 1);

        let event = TaskExecutionCompleted::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
            Duration::from_millis(100),
            None,
        );
        dispatcher.publish_task_execution_completed(&event);
        assert_eq!(listener.count(), 2);
    }

    #[test]
    fn test_multiple_listeners() {
        let dispatcher = EventDispatcher::new();
        let listener1 = Arc::new(CountingListener::new());
        let listener2 = Arc::new(CountingListener::new());

        dispatcher.register(listener1.clone());
        dispatcher.register(listener2.clone());

        let event = PollStarted::new("test_task", "worker-1", 10);
        dispatcher.publish_poll_started(&event);

        assert_eq!(listener1.count(), 1);
        assert_eq!(listener2.count(), 1);
    }
}
