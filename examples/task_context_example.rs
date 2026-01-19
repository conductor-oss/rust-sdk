//! Task Context Example
//!
//! Demonstrates TaskContext for accessing task metadata and poll_count.
//!
//! What it shows:
//! - Using task.context() shorthand (new recommended approach)
//! - Using TaskContext::from_task() (alternative approach)
//! - Direct helper methods on Task (task.task_id(), task.is_first_poll(), etc.)
//! - Checking retry_count
//! - Accessing task metadata (task_id, workflow_instance_id, etc.)
//! - Implementing long-running tasks with progress tracking
//! - Using is_first_poll() and is_retry() helpers
//!
//! For the #[worker] macro approach with TaskContext, see worker_macro_example.rs
//!
//! Run with: cargo run --example task_context_example
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, TaskInProgress, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
// Note: TaskContext is accessed via task.context() - no need to import directly
use std::time::Duration;
use tracing::info;

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

    // Create task handler with workers
    let mut handler = TaskHandler::new(config.clone())?;

    // ==============================
    // Worker 1: Demonstrates task.context() shorthand (RECOMMENDED)
    // ==============================
    let context_demo_worker = FnWorker::new("context_demo", |task: Task| async move {
        // NEW: Use task.context() shorthand - simplest approach
        let ctx = task.context();

        // Log all context information
        info!("=== TaskContext Information (using task.context()) ===");
        info!("  task_id: {}", ctx.task_id());
        info!("  workflow_instance_id: {}", ctx.workflow_instance_id());
        info!("  task_type: {}", ctx.task_type());
        info!("  reference_task_name: {}", ctx.reference_task_name());
        info!("  poll_count: {}", ctx.poll_count());
        info!("  retry_count: {}", ctx.retry_count());
        info!("  iteration: {}", ctx.iteration());
        info!("  is_first_poll: {}", ctx.is_first_poll());
        info!("  is_retry: {}", ctx.is_retry());

        if let Some(domain) = ctx.domain() {
            info!("  domain: {}", domain);
        }
        if let Some(correlation_id) = ctx.correlation_id() {
            info!("  correlation_id: {}", correlation_id);
        }

        Ok(WorkerOutput::completed_with_result(serde_json::json!({
            "poll_count": ctx.poll_count(),
            "retry_count": ctx.retry_count(),
            "is_first_poll": ctx.is_first_poll(),
            "task_id": ctx.task_id()
        })))
    })
    .with_thread_count(5);

    // ==============================
    // Worker 1b: Using direct Task helper methods
    // ==============================
    let direct_methods_worker = FnWorker::new("direct_methods_demo", |task: Task| async move {
        // NEW: Direct helper methods on Task - no need to create TaskContext
        info!("=== Direct Task Methods ===");
        info!("  task.task_id(): {}", task.task_id());
        info!(
            "  task.workflow_instance_id(): {}",
            task.workflow_instance_id()
        );
        info!("  task.poll_count(): {}", task.poll_count());
        info!("  task.retry_count(): {}", task.retry_count());
        info!("  task.is_first_poll(): {}", task.is_first_poll());
        info!("  task.is_retry(): {}", task.is_retry());

        // For simple cases, you don't need TaskContext at all!
        if task.is_first_poll() {
            info!("This is the first poll for task {}", task.task_id());
        }

        Ok(WorkerOutput::completed_with_result(serde_json::json!({
            "task_id": task.task_id(),
            "poll_count": task.poll_count()
        })))
    })
    .with_thread_count(5);

    // ==============================
    // Worker 2: Long-running task using poll_count
    // ==============================
    let progress_worker = FnWorker::new("progress_task", |task: Task| async move {
        // Using task.context() shorthand
        let ctx = task.context();
        let poll_count = ctx.poll_count();

        info!(
            "[Progress Worker] Poll #{} for task {}",
            poll_count,
            ctx.task_id()
        );

        // Simulate different behavior based on poll count
        if ctx.is_first_poll() {
            info!("  -> First poll - initializing task");

            // Return IN_PROGRESS to be polled again
            Ok(WorkerOutput::InProgress(
                TaskInProgress::new(2) // Callback after 2 seconds
                    .with_output_value("status", "initialized")
                    .with_output_value("progress", 0),
            ))
        } else if poll_count < 5 {
            let progress = poll_count * 20;
            info!("  -> Processing... {}% complete", progress);

            Ok(WorkerOutput::InProgress(
                TaskInProgress::new(2)
                    .with_output_value("status", "processing")
                    .with_output_value("progress", progress),
            ))
        } else {
            info!("  -> Task complete after {} polls!", poll_count);

            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "status": "completed",
                "total_polls": poll_count,
                "message": format!("Completed after {} iterations", poll_count)
            })))
        }
    })
    .with_thread_count(5);

    // ==============================
    // Worker 3: Retry-aware worker
    // ==============================
    let retry_aware_worker = FnWorker::new("retry_aware_task", |task: Task| async move {
        // Using direct Task methods - simplest for basic checks
        if task.is_retry() {
            info!(
                "[Retry Worker] This is retry attempt #{} for task {}",
                task.retry_count(),
                task.task_id()
            );

            // Maybe do something different on retry
            // e.g., use a fallback service, increase timeout, etc.
            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "status": "succeeded_on_retry",
                "retry_count": task.retry_count()
            })))
        } else {
            info!("[Retry Worker] First attempt for task {}", task.task_id());

            // Simulate a task that might fail on first attempt
            // In a real scenario, this could be a flaky external service
            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "status": "succeeded_first_try",
                "retry_count": 0
            })))
        }
    })
    .with_thread_count(5);

    // ==============================
    // Worker 4: Conditional logic based on poll_count
    // ==============================
    let conditional_worker = FnWorker::new("conditional_task", |task: Task| async move {
        let ctx = task.context();

        // Get any previous output that might have been set
        let previous_state: String = task
            .get_input("state")
            .unwrap_or_else(|| "start".to_string());

        info!(
            "[Conditional Worker] Poll {} - State: {}",
            ctx.poll_count(),
            previous_state
        );

        match (ctx.poll_count(), previous_state.as_str()) {
            (0, _) => {
                info!("  -> Starting workflow state machine");
                Ok(WorkerOutput::InProgress(
                    TaskInProgress::new(1)
                        .with_output_value("state", "step1")
                        .with_output_value("message", "Initialized"),
                ))
            }
            (1, "step1") => {
                info!("  -> Executing step 1");
                Ok(WorkerOutput::InProgress(
                    TaskInProgress::new(1)
                        .with_output_value("state", "step2")
                        .with_output_value("message", "Step 1 complete"),
                ))
            }
            (2, "step2") => {
                info!("  -> Executing step 2");
                Ok(WorkerOutput::InProgress(
                    TaskInProgress::new(1)
                        .with_output_value("state", "step3")
                        .with_output_value("message", "Step 2 complete"),
                ))
            }
            _ => {
                info!("  -> Final step - completing task");
                Ok(WorkerOutput::completed_with_result(serde_json::json!({
                    "state": "complete",
                    "message": "All steps finished",
                    "total_polls": ctx.poll_count()
                })))
            }
        }
    })
    .with_thread_count(5);

    // Add all workers
    handler.add_worker(context_demo_worker);
    handler.add_worker(direct_methods_worker);
    handler.add_worker(progress_worker);
    handler.add_worker(retry_aware_worker);
    handler.add_worker(conditional_worker);

    // Start the handler
    info!("Starting task handler...");
    handler.start().await?;

    // Execute test workflow
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("task_context_demo")
        .with_version(1)
        .with_correlation_id("task_context_test_123");

    info!("Starting workflow...");
    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Workflow started: {}", workflow_id);
    info!("View at: {}", config.execution_url(&workflow_id));

    // Wait for workflow to complete (it has long-running tasks)
    info!("Waiting for workflow to complete (up to 30 seconds)...");
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let workflow = workflow_client.get_workflow(&workflow_id, false).await?;

        if workflow.is_terminal() {
            info!("Workflow completed with status: {:?}", workflow.status);
            if let Some(output) = workflow.output.get("context_result") {
                info!("Context demo output: {}", output);
            }
            if let Some(output) = workflow.output.get("progress_result") {
                info!("Progress task output: {}", output);
            }
            break;
        }
    }

    // Keep running for manual testing
    info!("\nWorkers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("task_context_demo")
        .with_description("Demonstrates TaskContext usage")
        .with_version(1)
        // Task 1: Using task.context() - recommended approach
        .with_task(WorkflowTask::simple("context_demo", "context_ref"))
        // Task 1b: Using direct Task methods
        .with_task(WorkflowTask::simple("direct_methods_demo", "direct_ref"))
        // Task 2: Progress tracking with poll_count
        .with_task(WorkflowTask::simple("progress_task", "progress_ref"))
        // Task 3: Retry-aware behavior
        .with_task(WorkflowTask::simple("retry_aware_task", "retry_ref"))
        // Task 4: Conditional state machine
        .with_task(
            WorkflowTask::simple("conditional_task", "conditional_ref")
                .with_input_param("state", "${conditional_ref.output.state}"),
        )
        // Outputs
        .with_output_param("context_result", "${context_ref.output}")
        .with_output_param("direct_result", "${direct_ref.output}")
        .with_output_param("progress_result", "${progress_ref.output}")
        .with_output_param("retry_result", "${retry_ref.output}")
        .with_output_param("conditional_result", "${conditional_ref.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
