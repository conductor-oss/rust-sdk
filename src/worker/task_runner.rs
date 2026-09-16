// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::client::TaskClient;
use crate::configuration::{resolve_worker_config, WorkerConfig};
use crate::error::Result;
use crate::events::{
    exception_label, EventDispatcher, PollCompleted, PollFailure, PollSkippedPaused, PollStarted,
    TaskExecutionCompleted, TaskExecutionFailure, TaskExecutionStarted, TaskUpdateCompleted,
    TaskUpdateFailure, ThreadUncaughtException,
};
use crate::models::Task;

use super::{Worker, WorkerOutput};

/// Result of a task execution attempt, including panics.
///
/// Returned by [`TaskRunner::execute_catching_panic`] so the caller can
/// handle success, regular errors, and panics without accessing any
/// state that was inside the `AssertUnwindSafe` boundary.
enum TaskOutcome {
    Ok,
    Err(crate::error::ConductorError),
    Panic(String),
}

/// Task runner for a single worker type.
pub struct TaskRunner {
    worker: Arc<dyn Worker>,
    task_client: TaskClient,
    config: Arc<WorkerConfig>,
    event_dispatcher: EventDispatcher,

    // Control state
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,

    // Polling state
    consecutive_empty_polls: Arc<AtomicU64>,
    last_poll_time: Arc<parking_lot::Mutex<Instant>>,

    // Concurrency control - use atomic counter instead of HashSet for better performance
    semaphore: Arc<Semaphore>,
    /// Count of tasks currently being executed (after semaphore acquired).
    active_task_count: Arc<AtomicUsize>,
    /// Set of task IDs currently in flight (for debugging/monitoring).
    running_tasks: Arc<parking_lot::Mutex<HashSet<String>>>,
    /// Count of spawned tasks (including those waiting for semaphore).
    spawned_task_count: Arc<AtomicUsize>,
}

impl TaskRunner {
    /// Create a new task runner.
    pub fn new(
        worker: Arc<dyn Worker>,
        task_client: TaskClient,
        event_dispatcher: EventDispatcher,
    ) -> Self {
        // Resolve configuration from environment
        let defaults = WorkerConfig {
            task_definition_name: worker.task_definition_name().to_owned(),
            poll_interval: Duration::from_millis(worker.poll_interval_millis()),
            domain: worker.domain().map(std::borrow::ToOwned::to_owned),
            worker_id: worker.identity(),
            thread_count: worker.thread_count(),
            ..Default::default()
        };

        let config = resolve_worker_config(worker.task_definition_name(), defaults);

        info!(
            task_type = %config.task_definition_name,
            worker_id = %config.worker_id,
            thread_count = config.thread_count,
            poll_interval_ms = config.poll_interval.as_millis(),
            domain = ?config.domain,
            paused = config.paused,
            "Task runner initialized"
        );

        let paused = config.paused;
        let thread_count = config.thread_count;

        Self {
            worker,
            task_client,
            config: Arc::new(config),
            event_dispatcher,
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(paused)),
            consecutive_empty_polls: Arc::new(AtomicU64::new(0)),
            last_poll_time: Arc::new(parking_lot::Mutex::new(Instant::now())),
            semaphore: Arc::new(Semaphore::new(thread_count)),
            active_task_count: Arc::new(AtomicUsize::new(0)),
            running_tasks: Arc::new(parking_lot::Mutex::new(HashSet::new())),
            spawned_task_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the task type this runner handles.
    #[must_use]
    pub fn task_type(&self) -> &str {
        &self.config.task_definition_name
    }

    /// Get the worker configuration.
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Get the number of currently active tasks (executing, not waiting for semaphore).
    #[must_use]
    pub fn active_task_count(&self) -> usize {
        self.active_task_count.load(Ordering::SeqCst)
    }

    /// Get the number of spawned tasks (including those waiting for semaphore).
    #[must_use]
    pub fn spawned_task_count(&self) -> usize {
        self.spawned_task_count.load(Ordering::SeqCst)
    }

    /// Check if the runner is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if the runner is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Pause the runner.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner paused");
    }

    /// Resume the runner.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner resumed");
    }

    /// Stop the runner.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner stopped");
    }

    /// Run the polling loop.
    pub async fn run(&self) {
        self.running.store(true, Ordering::SeqCst);

        info!(
            task_type = %self.config.task_definition_name,
            "Starting task runner polling loop"
        );

        while self.running.load(Ordering::SeqCst) {
            if let Err(e) = self.run_once().await {
                error!(
                    task_type = %self.config.task_definition_name,
                    error = %e,
                    "Error in polling loop"
                );
            }
        }

        // Wait for in-flight tasks to complete (graceful shutdown)
        self.wait_for_tasks_to_complete().await;

        info!(
            task_type = %self.config.task_definition_name,
            "Task runner polling loop ended"
        );
    }

    /// Wait for all spawned tasks to complete (used during shutdown).
    async fn wait_for_tasks_to_complete(&self) {
        let shutdown_timeout = Duration::from_secs(30);
        let start = Instant::now();

        while self.spawned_task_count.load(Ordering::SeqCst) > 0 {
            if start.elapsed() > shutdown_timeout {
                let remaining = self.spawned_task_count.load(Ordering::SeqCst);
                warn!(
                    task_type = %self.config.task_definition_name,
                    remaining_tasks = remaining,
                    "Shutdown timeout reached, {} tasks still in flight",
                    remaining
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Run one iteration of the polling loop.
    async fn run_once(&self) -> Result<()> {
        // Check if paused
        if self.paused.load(Ordering::SeqCst) {
            self.event_dispatcher
                .publish_poll_skipped_paused(&PollSkippedPaused::new(
                    &self.config.task_definition_name,
                    &self.config.worker_id,
                ));
            tokio::time::sleep(Duration::from_millis(100)).await;
            return Ok(());
        }

        // Calculate available capacity based on ACTIVE tasks (those that have acquired semaphore)
        // This is more accurate than tracking spawned tasks since semaphore controls actual concurrency
        let active_count = self.active_task_count.load(Ordering::SeqCst);
        let available_slots = self.config.thread_count.saturating_sub(active_count);

        if available_slots == 0 {
            // At capacity, wait briefly
            tokio::time::sleep(Duration::from_millis(1)).await;
            return Ok(());
        }

        // Apply adaptive backoff
        let empty_polls = self.consecutive_empty_polls.load(Ordering::SeqCst);
        if empty_polls > 0 {
            let backoff = Duration::from_millis(1 << empty_polls.min(10));
            let backoff = backoff.min(self.config.poll_interval);

            let elapsed = self.last_poll_time.lock().elapsed();
            if elapsed < backoff {
                tokio::time::sleep(backoff.checked_sub(elapsed).unwrap_or_default()).await;
            }
        }

        // Poll for tasks
        let poll_start = Instant::now();

        // Publish poll started event
        self.event_dispatcher
            .publish_poll_started(&PollStarted::new(
                &self.config.task_definition_name,
                &self.config.worker_id,
                available_slots,
            ));

        let poll_result = self
            .task_client
            .batch_poll(
                &self.config.task_definition_name,
                Some(&self.config.worker_id),
                self.config.domain.as_deref(),
                available_slots,
                self.config.poll_timeout,
            )
            .await;

        let poll_duration = poll_start.elapsed();
        *self.last_poll_time.lock() = Instant::now();

        match poll_result {
            Ok(tasks) => {
                // Publish poll completed event
                self.event_dispatcher
                    .publish_poll_completed(&PollCompleted::new(
                        &self.config.task_definition_name,
                        &self.config.worker_id,
                        poll_duration,
                        tasks.len(),
                    ));

                if tasks.is_empty() {
                    self.consecutive_empty_polls.fetch_add(1, Ordering::SeqCst);
                } else {
                    self.consecutive_empty_polls.store(0, Ordering::SeqCst);

                    // Process tasks
                    for task in tasks {
                        self.spawn_task_execution(task);
                    }
                }
            }
            Err(e) => {
                let exception = exception_label(&e);

                self.event_dispatcher
                    .publish_poll_failure(&PollFailure::new(
                        &self.config.task_definition_name,
                        &self.config.worker_id,
                        poll_duration,
                        e.to_string(),
                        exception,
                    ));

                self.consecutive_empty_polls.fetch_add(1, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    /// Spawn task execution in background.
    ///
    /// This method correctly handles the semaphore acquisition order to avoid
    /// race conditions in capacity calculation:
    /// 1. Increment spawned count (for shutdown tracking)
    /// 2. Spawn task
    /// 3. Acquire semaphore (wait if at capacity)
    /// 4. Increment active count (now executing)
    /// 5. Track task ID in `running_tasks`
    /// 6. Execute task
    /// 7. Decrement active count and remove from `running_tasks`
    /// 8. Decrement spawned count
    fn spawn_task_execution(&self, task: Task) {
        let task_id = task.task_id.clone();

        // Increment spawned task count for shutdown tracking
        self.spawned_task_count.fetch_add(1, Ordering::SeqCst);

        let worker = Arc::clone(&self.worker);
        let task_client = self.task_client.clone();
        let event_dispatcher = self.event_dispatcher.clone();
        let config = Arc::clone(&self.config);
        let semaphore = Arc::clone(&self.semaphore);
        let active_task_count = Arc::clone(&self.active_task_count);
        let running_tasks = Arc::clone(&self.running_tasks);
        let spawned_task_count = Arc::clone(&self.spawned_task_count);

        let task_type = self.config.task_definition_name.clone();

        tokio::spawn(async move {
            // Acquire semaphore permit FIRST - this is the actual concurrency control
            let Ok(_permit) = semaphore.acquire().await else {
                // Semaphore was closed (shouldn't happen in normal operation)
                error!(task_id = %task_id, "Semaphore closed, dropping task");
                spawned_task_count.fetch_sub(1, Ordering::SeqCst);
                return;
            };

            // NOW increment active count and track the task
            // This ensures capacity calculation is accurate
            active_task_count.fetch_add(1, Ordering::SeqCst);
            running_tasks.lock().insert(task_id.clone());

            // `worker`, `config`, and `task` are moved into the panic-catching
            // boundary and cannot be accessed in the cleanup code below.
            let outcome =
                Self::execute_catching_panic(worker, &task_client, &event_dispatcher, config, task)
                    .await;

            // Cleanup: only atomics, locks, and event_dispatcher are accessible
            running_tasks.lock().remove(&task_id);
            active_task_count.fetch_sub(1, Ordering::SeqCst);
            spawned_task_count.fetch_sub(1, Ordering::SeqCst);

            match outcome {
                TaskOutcome::Ok => {}
                TaskOutcome::Err(e) => {
                    error!(
                        task_id = %task_id,
                        error = %e,
                        "Task execution failed"
                    );
                }
                TaskOutcome::Panic(panic_msg) => {
                    error!(
                        task_id = %task_id,
                        task_type = %task_type,
                        panic_message = %panic_msg,
                        "Uncaught panic in worker task"
                    );
                    event_dispatcher.publish_thread_uncaught_exception(
                        &ThreadUncaughtException::new(&task_type, "Panic"),
                    );
                }
            }
        });
    }

    /// Execute a task inside a panic-catching boundary.
    ///
    /// `worker`, `config`, and `task` are consumed so that the caller
    /// cannot access them after a potential panic — only the returned
    /// [`TaskOutcome`] carries the information needed for logging and
    /// event publishing.
    async fn execute_catching_panic(
        worker: Arc<dyn Worker>,
        task_client: &TaskClient,
        event_dispatcher: &EventDispatcher,
        config: Arc<WorkerConfig>,
        task: Task,
    ) -> TaskOutcome {
        match AssertUnwindSafe(Self::execute_and_update_task(
            &worker,
            task_client,
            event_dispatcher,
            &config,
            task,
        ))
        .catch_unwind()
        .await
        {
            Ok(Ok(())) => TaskOutcome::Ok,
            Ok(Err(e)) => TaskOutcome::Err(e),
            Err(panic_payload) => {
                let msg = panic_payload
                    .downcast_ref::<String>()
                    .map(std::string::String::as_str)
                    .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                    .unwrap_or("<non-string panic>");
                TaskOutcome::Panic(msg.to_owned())
            }
        }
    }

    /// Execute a task and update the result.
    ///
    /// Takes ownership of the Task to wrap it in Arc, avoiding clones
    /// when passing to workers.
    async fn execute_and_update_task(
        worker: &Arc<dyn Worker>,
        task_client: &TaskClient,
        event_dispatcher: &EventDispatcher,
        config: &Arc<WorkerConfig>,
        task: Task,
    ) -> Result<()> {
        // Wrap task in Arc once - this is the only allocation
        let task = Arc::new(task);

        let task_id = &task.task_id;
        let task_type = &task.task_type;
        let workflow_id = &task.workflow_instance_id;

        debug!(
            task_id = %task_id,
            task_type = %task_type,
            "Executing task"
        );

        // Publish execution started event
        event_dispatcher.publish_task_execution_started(&TaskExecutionStarted::new(
            task_type,
            task_id,
            workflow_id,
            &config.worker_id,
        ));

        let exec_start = Instant::now();

        // Start a lease-extension heartbeat alongside execution, if configured -- matches
        // python-sdk's `LeaseManager`, translated to a per-task spawned tokio task (cheap here,
        // unlike an OS thread) rather than a shared background-thread manager.
        let heartbeat_handle = Self::maybe_spawn_lease_heartbeat(task_client, &task, config);

        // Execute the worker - pass reference to avoid clone in worker trait
        let exec_result = worker.execute(&task).await;
        let exec_duration = exec_start.elapsed();

        if let Some(handle) = heartbeat_handle {
            handle.abort();
        }

        // Convert result to TaskResult
        let task_result = match exec_result {
            Ok(output) => {
                let output_size = match &output {
                    WorkerOutput::Completed(data) => {
                        serde_json::to_string(data).map(|s| s.len()).ok()
                    }
                    _ => None,
                };

                // Publish execution completed event
                event_dispatcher.publish_task_execution_completed(&TaskExecutionCompleted::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    exec_duration,
                    output_size,
                ));

                output.into_task_result(&task, &config.worker_id)
            }
            Err(e) => {
                let error_msg = e.to_string();
                let exception = exception_label(&e);
                let is_retryable = e.is_retryable();

                event_dispatcher.publish_task_execution_failure(&TaskExecutionFailure::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    exec_duration,
                    &error_msg,
                    exception,
                    is_retryable,
                ));

                WorkerOutput::Failed(error_msg).into_task_result(&task, &config.worker_id)
            }
        };

        // Update task with retry
        let update_start = Instant::now();
        match task_client.update_task_with_retry(&task_result, 4).await {
            Ok(_) => {
                let update_duration = update_start.elapsed();
                debug!(task_id = %task_id, "Task updated successfully");

                event_dispatcher.publish_task_update_completed(&TaskUpdateCompleted::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    update_duration,
                ));
            }
            Err(e) => {
                let update_duration = update_start.elapsed();
                error!(task_id = %task_id, error = %e, "Failed to update task after retries");
                let exception = exception_label(&e);

                event_dispatcher.publish_task_update_failure(&TaskUpdateFailure::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    update_duration,
                    e.to_string(),
                    exception,
                    4,
                ));
            }
        }

        Ok(())
    }

    /// Start a background heartbeat loop for `task`, if lease extension is enabled and the
    /// task's `response_timeout_seconds` makes it worthwhile. Returns `None` (spawning nothing)
    /// when disabled, matching python-sdk's `_track_lease`'s early-return conditions exactly.
    fn maybe_spawn_lease_heartbeat(
        task_client: &TaskClient,
        task: &Arc<Task>,
        config: &Arc<WorkerConfig>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        if !config.lease_extend_enabled {
            return None;
        }
        if task.response_timeout_seconds <= 0 {
            return None;
        }
        let interval_secs = task.response_timeout_seconds as f64 * config.lease_extend_threshold;
        // Matches python's `LeaseManager.track`: an interval under a second isn't worth
        // scheduling a repeating heartbeat for.
        if interval_secs < 1.0 {
            return None;
        }

        let task_client = task_client.clone();
        let task_id = task.task_id.clone();
        let workflow_instance_id = task.workflow_instance_id.clone();
        let interval = Duration::from_secs_f64(interval_secs);

        Some(tokio::spawn(Self::send_lease_heartbeats(
            task_client,
            task_id,
            workflow_instance_id,
            interval,
        )))
    }

    /// Send a lease-extension heartbeat every `interval`, starting `interval` after this is
    /// spawned (not immediately) -- matches python's `LeaseManager`, which arms
    /// `last_heartbeat_time` at `track()` time and only fires once that much time has elapsed.
    /// Runs until the caller aborts the returned `JoinHandle` (when the task finishes), which is
    /// the only way this loop ends.
    #[expect(clippy::infinite_loop)]
    async fn send_lease_heartbeats(
        task_client: TaskClient,
        task_id: String,
        workflow_instance_id: String,
        interval: Duration,
    ) {
        // Matches python's `LeaseManager._send_heartbeat`: a short, fixed retry count with fast
        // backoff -- deliberately not `TaskClient::update_task_with_retry`'s 10/20/30s schedule,
        // which is sized for terminal completion updates, not a fast-repeating keep-alive that
        // will just get another chance at the next tick anyway.
        const LEASE_EXTEND_RETRY_COUNT: u32 = 3;

        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        loop {
            ticker.tick().await;
            let heartbeat = crate::models::TaskResult {
                task_id: task_id.clone(),
                workflow_instance_id: workflow_instance_id.clone(),
                status: crate::models::TaskResultStatus::InProgress,
                extend_lease: true,
                ..Default::default()
            };

            for attempt in 0..LEASE_EXTEND_RETRY_COUNT {
                match task_client.update_task(&heartbeat).await {
                    Ok(_) => {
                        debug!(task_id = %task_id, "Extended lease");
                        break;
                    }
                    Err(e) if attempt + 1 < LEASE_EXTEND_RETRY_COUNT => {
                        warn!(task_id = %task_id, error = %e, attempt, "Lease heartbeat failed, retrying");
                        tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt + 2)))
                            .await;
                    }
                    Err(e) => {
                        error!(task_id = %task_id, error = %e, "Failed to extend lease after {LEASE_EXTEND_RETRY_COUNT} attempts");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::Configuration;
    use crate::http::ApiClient;
    use async_trait::async_trait;

    struct TestWorker;

    #[async_trait]
    impl Worker for TestWorker {
        fn task_definition_name(&self) -> &'static str {
            "test_task"
        }

        async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
            let name = task
                .get_input_string("name")
                .unwrap_or_else(|| "World".to_owned());
            Ok(WorkerOutput::completed_with_result(format!(
                "Hello, {name}!"
            )))
        }

        fn thread_count(&self) -> usize {
            5
        }
    }

    #[test]
    fn test_task_runner_config() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let task_client = TaskClient::new(api);
        let worker = Arc::new(TestWorker);
        let dispatcher = EventDispatcher::new();

        let runner = TaskRunner::new(worker, task_client, dispatcher);

        assert_eq!(runner.task_type(), "test_task");
        assert_eq!(runner.config().thread_count, 5);
    }

    fn test_task_client() -> TaskClient {
        let config = Configuration::new("http://localhost:8080/api");
        TaskClient::new(ApiClient::new(config).unwrap())
    }

    #[test]
    fn test_lease_heartbeat_not_spawned_when_disabled() {
        let task_client = test_task_client();
        let task = Arc::new(Task {
            response_timeout_seconds: 30,
            ..Default::default()
        });
        let config = Arc::new(WorkerConfig::new("t").with_lease_extend_enabled(false));

        let handle = TaskRunner::maybe_spawn_lease_heartbeat(&task_client, &task, &config);
        assert!(handle.is_none());
    }

    #[test]
    fn test_lease_heartbeat_not_spawned_without_response_timeout() {
        let task_client = test_task_client();
        let task = Arc::new(Task {
            response_timeout_seconds: 0,
            ..Default::default()
        });
        let config = Arc::new(WorkerConfig::new("t").with_lease_extend_enabled(true));

        let handle = TaskRunner::maybe_spawn_lease_heartbeat(&task_client, &task, &config);
        assert!(handle.is_none());
    }

    #[test]
    fn test_lease_heartbeat_not_spawned_when_interval_too_short() {
        let task_client = test_task_client();
        // 1s timeout * 0.8 threshold = 0.8s, matching python's "< 1 second" skip.
        let task = Arc::new(Task {
            response_timeout_seconds: 1,
            ..Default::default()
        });
        let config = Arc::new(WorkerConfig::new("t").with_lease_extend_enabled(true));

        let handle = TaskRunner::maybe_spawn_lease_heartbeat(&task_client, &task, &config);
        assert!(handle.is_none());
    }

    #[tokio::test]
    async fn test_lease_heartbeat_spawned_when_enabled_and_worthwhile() {
        let task_client = test_task_client();
        let task = Arc::new(Task {
            response_timeout_seconds: 30,
            ..Default::default()
        });
        let config = Arc::new(
            WorkerConfig::new("t")
                .with_lease_extend_enabled(true)
                .with_lease_extend_threshold(0.8),
        );

        let handle = TaskRunner::maybe_spawn_lease_heartbeat(&task_client, &task, &config);
        assert!(handle.is_some());
        // Abort immediately -- this test only checks that a heartbeat loop gets scheduled at
        // all, not its actual network behavior (which needs a live/mock server and real time).
        handle.unwrap().abort();
    }
}
