//! Task runner for polling and executing tasks
//!
//! This implements the core polling loop with:
//! - Dynamic batch polling based on capacity
//! - Adaptive backoff when queue is empty
//! - Concurrent task execution with semaphore
//! - Event publishing for metrics
//! - Graceful shutdown with task tracking

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::client::TaskClient;
use crate::configuration::{resolve_worker_config, WorkerConfig};
use crate::error::Result;
use crate::events::{
    EventDispatcher, PollCompleted, PollFailure, PollStarted, TaskExecutionCompleted,
    TaskExecutionFailure, TaskExecutionStarted, TaskUpdateFailure,
};
use crate::models::Task;

use super::{Worker, WorkerOutput};

/// Task runner for a single worker type
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
    /// Count of tasks currently being executed (after semaphore acquired)
    active_task_count: Arc<AtomicUsize>,
    /// Set of task IDs currently in flight (for debugging/monitoring)
    running_tasks: Arc<parking_lot::Mutex<HashSet<String>>>,
    /// Count of spawned tasks (including those waiting for semaphore)
    spawned_task_count: Arc<AtomicUsize>,
}

impl TaskRunner {
    /// Create a new task runner
    pub fn new(
        worker: Arc<dyn Worker>,
        task_client: TaskClient,
        event_dispatcher: EventDispatcher,
    ) -> Self {
        // Resolve configuration from environment
        let defaults = WorkerConfig {
            task_definition_name: worker.task_definition_name().to_string(),
            poll_interval: Duration::from_millis(worker.poll_interval_millis()),
            domain: worker.domain().map(|s| s.to_string()),
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

    /// Get the task type this runner handles
    pub fn task_type(&self) -> &str {
        &self.config.task_definition_name
    }

    /// Get the worker configuration
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// Get the number of currently active tasks (executing, not waiting for semaphore)
    pub fn active_task_count(&self) -> usize {
        self.active_task_count.load(Ordering::SeqCst)
    }

    /// Get the number of spawned tasks (including those waiting for semaphore)
    pub fn spawned_task_count(&self) -> usize {
        self.spawned_task_count.load(Ordering::SeqCst)
    }

    /// Check if the runner is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if the runner is paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Pause the runner
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner paused");
    }

    /// Resume the runner
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner resumed");
    }

    /// Stop the runner
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!(task_type = %self.config.task_definition_name, "Task runner stopped");
    }

    /// Run the polling loop
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

    /// Wait for all spawned tasks to complete (used during shutdown)
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

    /// Run one iteration of the polling loop
    async fn run_once(&self) -> Result<()> {
        // Check if paused
        if self.paused.load(Ordering::SeqCst) {
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
                tokio::time::sleep(backoff - elapsed).await;
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
                // Publish poll failure event
                self.event_dispatcher
                    .publish_poll_failure(&PollFailure::new(
                        &self.config.task_definition_name,
                        &self.config.worker_id,
                        poll_duration,
                        e.to_string(),
                    ));

                self.consecutive_empty_polls.fetch_add(1, Ordering::SeqCst);
            }
        }

        Ok(())
    }

    /// Spawn task execution in background
    ///
    /// This method correctly handles the semaphore acquisition order to avoid
    /// race conditions in capacity calculation:
    /// 1. Increment spawned count (for shutdown tracking)
    /// 2. Spawn task
    /// 3. Acquire semaphore (wait if at capacity)
    /// 4. Increment active count (now executing)
    /// 5. Track task ID in running_tasks
    /// 6. Execute task
    /// 7. Decrement active count and remove from running_tasks
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

        tokio::spawn(async move {
            // Acquire semaphore permit FIRST - this is the actual concurrency control
            let _permit = match semaphore.acquire().await {
                Ok(permit) => permit,
                Err(_) => {
                    // Semaphore was closed (shouldn't happen in normal operation)
                    error!(task_id = %task_id, "Semaphore closed, dropping task");
                    spawned_task_count.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            };

            // NOW increment active count and track the task
            // This ensures capacity calculation is accurate
            active_task_count.fetch_add(1, Ordering::SeqCst);
            running_tasks.lock().insert(task_id.clone());

            let result = Self::execute_and_update_task(
                &worker,
                &task_client,
                &event_dispatcher,
                &config,
                task,
            )
            .await;

            // Cleanup: remove from tracking
            running_tasks.lock().remove(&task_id);
            active_task_count.fetch_sub(1, Ordering::SeqCst);
            spawned_task_count.fetch_sub(1, Ordering::SeqCst);

            if let Err(e) = result {
                error!(
                    task_id = %task_id,
                    error = %e,
                    "Task execution failed"
                );
            }
        });
    }

    /// Execute a task and update the result
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

        // Execute the worker - pass reference to avoid clone in worker trait
        let exec_result = worker.execute(&task).await;
        let exec_duration = exec_start.elapsed();

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

                // Publish execution failure event
                event_dispatcher.publish_task_execution_failure(&TaskExecutionFailure::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    exec_duration,
                    &error_msg,
                    e.is_retryable(),
                ));

                WorkerOutput::Failed(error_msg).into_task_result(&task, &config.worker_id)
            }
        };

        // Update task with retry
        match task_client.update_task_with_retry(&task_result, 4).await {
            Ok(_) => {
                debug!(task_id = %task_id, "Task updated successfully");
            }
            Err(e) => {
                error!(task_id = %task_id, error = %e, "Failed to update task after retries");

                // Publish task update failure event
                event_dispatcher.publish_task_update_failure(&TaskUpdateFailure::new(
                    task_type,
                    task_id,
                    workflow_id,
                    &config.worker_id,
                    e.to_string(),
                    4,
                ));
            }
        }

        Ok(())
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
        fn task_definition_name(&self) -> &str {
            "test_task"
        }

        async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
            let name = task
                .get_input_string("name")
                .unwrap_or_else(|| "World".to_string());
            Ok(WorkerOutput::completed_with_result(format!(
                "Hello, {}!",
                name
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
}
