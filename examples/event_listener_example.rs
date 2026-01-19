//! Event Listener Example
//!
//! Demonstrates custom event listeners for monitoring worker activity.
//!
//! What it shows:
//! - Implementing TaskRunnerEventsListener trait
//! - Handling all event types (poll, execution, update)
//! - Custom logging and alerting
//! - SLA monitoring
//! - Error tracking
//!
//! Run with: cargo run --example event_listener_example
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    events::{
        PollCompleted, PollFailure, PollStarted, TaskExecutionCompleted, TaskExecutionFailure,
        TaskExecutionStarted, TaskRunnerEventsListener, TaskUpdateFailure,
    },
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Custom event listener that logs all events
struct LoggingListener {
    name: String,
}

impl LoggingListener {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl TaskRunnerEventsListener for LoggingListener {
    fn on_poll_started(&self, event: &PollStarted) {
        info!(
            "[{}] Poll started: task_type={}, worker_id={}, poll_count={}",
            self.name, event.task_type, event.worker_id, event.poll_count
        );
    }

    fn on_poll_completed(&self, event: &PollCompleted) {
        info!(
            "[{}] Poll completed: task_type={}, tasks_received={}, duration={:?}",
            self.name, event.task_type, event.tasks_received, event.duration
        );
    }

    fn on_poll_failure(&self, event: &PollFailure) {
        error!(
            "[{}] Poll FAILED: task_type={}, error={}, duration={:?}",
            self.name, event.task_type, event.error, event.duration
        );
    }

    fn on_task_execution_started(&self, event: &TaskExecutionStarted) {
        info!(
            "[{}] Task execution started: task_type={}, task_id={}, workflow_id={}",
            self.name, event.task_type, event.task_id, event.workflow_instance_id
        );
    }

    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        info!(
            "[{}] Task execution completed: task_type={}, task_id={}, duration={:?}, output_size={:?}",
            self.name, event.task_type, event.task_id, event.duration, event.output_size_bytes
        );
    }

    fn on_task_execution_failure(&self, event: &TaskExecutionFailure) {
        error!(
            "[{}] Task execution FAILED: task_type={}, task_id={}, error={}, is_retryable={}",
            self.name, event.task_type, event.task_id, event.error, event.is_retryable
        );
    }

    fn on_task_update_failure(&self, event: &TaskUpdateFailure) {
        error!(
            "[{}] Task update FAILED: task_type={}, task_id={}, error={}, retry_count={}",
            self.name, event.task_type, event.task_id, event.error, event.retry_count
        );
    }
}

/// SLA monitoring listener - alerts when tasks take too long
struct SLAMonitor {
    threshold_ms: u64,
    violations: AtomicU64,
}

impl SLAMonitor {
    fn new(threshold_ms: u64) -> Self {
        Self {
            threshold_ms,
            violations: AtomicU64::new(0),
        }
    }

    fn violation_count(&self) -> u64 {
        self.violations.load(Ordering::SeqCst)
    }
}

impl TaskRunnerEventsListener for SLAMonitor {
    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        let duration_ms = event.duration.as_millis() as u64;

        if duration_ms > self.threshold_ms {
            self.violations.fetch_add(1, Ordering::SeqCst);
            warn!(
                "[SLA VIOLATION] Task {} took {}ms (threshold: {}ms)",
                event.task_id, duration_ms, self.threshold_ms
            );
        }
    }
}

/// Statistics collector - tracks metrics without Prometheus
struct StatsCollector {
    polls: AtomicU64,
    tasks_received: AtomicU64,
    tasks_completed: AtomicU64,
    tasks_failed: AtomicU64,
    total_execution_time_ms: AtomicU64,
}

impl StatsCollector {
    fn new() -> Self {
        Self {
            polls: AtomicU64::new(0),
            tasks_received: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            total_execution_time_ms: AtomicU64::new(0),
        }
    }

    fn print_stats(&self) {
        let polls = self.polls.load(Ordering::SeqCst);
        let received = self.tasks_received.load(Ordering::SeqCst);
        let completed = self.tasks_completed.load(Ordering::SeqCst);
        let failed = self.tasks_failed.load(Ordering::SeqCst);
        let total_time = self.total_execution_time_ms.load(Ordering::SeqCst);

        let avg_time = if completed > 0 {
            total_time / completed
        } else {
            0
        };

        println!("\n=== Worker Statistics ===");
        println!("  Total polls: {}", polls);
        println!("  Tasks received: {}", received);
        println!("  Tasks completed: {}", completed);
        println!("  Tasks failed: {}", failed);
        println!("  Avg execution time: {}ms", avg_time);
        println!("=========================\n");
    }
}

impl TaskRunnerEventsListener for StatsCollector {
    fn on_poll_completed(&self, event: &PollCompleted) {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.tasks_received
            .fetch_add(event.tasks_received as u64, Ordering::SeqCst);
    }

    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        self.tasks_completed.fetch_add(1, Ordering::SeqCst);
        self.total_execution_time_ms
            .fetch_add(event.duration.as_millis() as u64, Ordering::SeqCst);
    }

    fn on_task_execution_failure(&self, _event: &TaskExecutionFailure) {
        self.tasks_failed.fetch_add(1, Ordering::SeqCst);
    }
}

/// Error alerting listener - could send to external system
struct ErrorAlerter {
    alert_count: AtomicU64,
}

impl ErrorAlerter {
    fn new() -> Self {
        Self {
            alert_count: AtomicU64::new(0),
        }
    }

    fn alert(&self, message: &str) {
        self.alert_count.fetch_add(1, Ordering::SeqCst);
        // In a real system, this could:
        // - Send to Slack/Teams
        // - Create a PagerDuty incident
        // - Log to external monitoring system
        error!(
            "[ALERT #{}] {}",
            self.alert_count.load(Ordering::SeqCst),
            message
        );
    }
}

impl TaskRunnerEventsListener for ErrorAlerter {
    fn on_poll_failure(&self, event: &PollFailure) {
        self.alert(&format!(
            "Poll failed for task_type={}: {}",
            event.task_type, event.error
        ));
    }

    fn on_task_execution_failure(&self, event: &TaskExecutionFailure) {
        if !event.is_retryable {
            self.alert(&format!(
                "Non-retryable task failure: task_id={}, error={}",
                event.task_id, event.error
            ));
        }
    }

    fn on_task_update_failure(&self, event: &TaskUpdateFailure) {
        self.alert(&format!(
            "Task update failed after {} retries: task_id={}, error={}",
            event.retry_count, event.task_id, event.error
        ));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("conductor=info".parse().unwrap()),
        )
        .init();

    // Load configuration
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config.clone())?;

    // Register workflow
    register_workflow(&client).await?;

    // Create task handler
    let mut handler = TaskHandler::new(config.clone())?;

    // ==============================
    // Register Event Listeners
    // ==============================

    // 1. Detailed logging listener
    handler.add_event_listener(Arc::new(LoggingListener::new("EventLog")));

    // 2. SLA monitoring (alert if task takes > 500ms)
    let sla_monitor = Arc::new(SLAMonitor::new(500));
    handler.add_event_listener(sla_monitor.clone());

    // 3. Statistics collector
    let stats = Arc::new(StatsCollector::new());
    handler.add_event_listener(stats.clone());

    // 4. Error alerter
    handler.add_event_listener(Arc::new(ErrorAlerter::new()));

    // ==============================
    // Create Workers
    // ==============================

    // Fast worker - should complete within SLA
    let fast_worker = FnWorker::new("fast_task", |_task: Task| async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(WorkerOutput::completed_with_result("fast task done"))
    })
    .with_thread_count(5);

    // Slow worker - will trigger SLA violation
    let slow_worker = FnWorker::new("slow_task", |_task: Task| async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        Ok(WorkerOutput::completed_with_result("slow task done"))
    })
    .with_thread_count(2);

    // Flaky worker - sometimes fails
    let flaky_worker = FnWorker::new("flaky_task", |task: Task| async move {
        let should_fail: bool = task.get_input("fail").unwrap_or(false);

        if should_fail {
            Ok(WorkerOutput::failed("Simulated failure"))
        } else {
            Ok(WorkerOutput::completed_with_result("flaky task succeeded"))
        }
    })
    .with_thread_count(3);

    // Add workers
    handler.add_worker(fast_worker);
    handler.add_worker(slow_worker);
    handler.add_worker(flaky_worker);

    // Start the handler
    info!("Starting task handler with event listeners...");
    handler.start().await?;

    println!("\n{}", "=".repeat(70));
    println!("Event Listener Example");
    println!("{}", "=".repeat(70));
    println!("\nRegistered Listeners:");
    println!("  1. LoggingListener - Logs all events");
    println!("  2. SLAMonitor - Alerts when tasks take > 500ms");
    println!("  3. StatsCollector - Collects execution statistics");
    println!("  4. ErrorAlerter - Sends alerts on failures");
    println!("\nWorkers:");
    println!("  - fast_task: Completes in ~100ms (within SLA)");
    println!("  - slow_task: Completes in ~800ms (SLA violation!)");
    println!("  - flaky_task: Sometimes fails");
    println!("{}", "=".repeat(70));

    // Execute test workflows
    let workflow_client = client.workflow_client();

    // Start workflow with fast task
    info!("\nStarting workflow with fast task...");
    let request = StartWorkflowRequest::new("event_listener_demo")
        .with_version(1)
        .with_input_value("task_type", "fast")
        .with_input_value("should_fail", false);
    let _ = workflow_client.start_workflow(&request).await?;

    // Start workflow with slow task (will trigger SLA)
    info!("Starting workflow with slow task (will trigger SLA violation)...");
    let request = StartWorkflowRequest::new("event_listener_demo")
        .with_version(1)
        .with_input_value("task_type", "slow")
        .with_input_value("should_fail", false);
    let _ = workflow_client.start_workflow(&request).await?;

    // Wait for tasks to complete
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Print statistics
    stats.print_stats();
    info!("SLA violations: {}", sla_monitor.violation_count());

    // Keep running
    info!("\nWorkers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    // Final statistics
    stats.print_stats();

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("event_listener_demo")
        .with_description("Demonstrates event listeners")
        .with_version(1)
        // Switch based on task_type input
        .with_task(
            WorkflowTask::switch("select_task", "$.task_type")
                .with_input_param("task_type", "${workflow.input.task_type}")
                .with_switch_case("fast", vec![WorkflowTask::simple("fast_task", "fast_ref")])
                .with_switch_case("slow", vec![WorkflowTask::simple("slow_task", "slow_ref")])
                .with_default_case(vec![WorkflowTask::simple("flaky_task", "flaky_ref")
                    .with_input_param("fail", "${workflow.input.should_fail}")]),
        )
        .with_output_param("result", "${select_task.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
