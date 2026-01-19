//! Comprehensive Worker Example
//!
//! Demonstrates both async workers and long-running task patterns.
//!
//! What it shows:
//! - Async workers for I/O-bound tasks (HTTP calls, database queries)
//! - Workers with different thread counts
//! - Long-running tasks with callback patterns
//! - Error handling in workers
//! - Worker output patterns (completed, failed, in_progress)
//!
//! Run with: cargo run --example worker_example
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, TaskDef, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
use std::collections::HashMap;
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

    // Register task definitions and workflow
    register_definitions(&client).await?;

    // Create and configure the task handler
    let mut handler = TaskHandler::new(config.clone())?;

    // ============================================================================
    // ASYNC WORKERS - I/O-Bound Tasks
    // ============================================================================

    // Worker 1: Fetch user data - simulates I/O-bound API call
    let fetch_user_worker = FnWorker::new("fetch_user_data", |task: Task| async move {
        let user_id = task
            .get_input_string("user_id")
            .unwrap_or_else(|| "unknown".to_string());

        info!("Fetching user data for user_id={}", user_id);

        // Simulate async HTTP call or database query
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mut output = HashMap::new();
        output.insert("user_id".to_string(), serde_json::json!(user_id));
        output.insert(
            "name".to_string(),
            serde_json::json!(format!("User {}", user_id)),
        );
        output.insert(
            "email".to_string(),
            serde_json::json!(format!("user{}@example.com", user_id)),
        );
        output.insert("status".to_string(), serde_json::json!("active"));

        info!("Successfully fetched user data for user_id={}", user_id);

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(50); // High concurrency for I/O-bound tasks

    // Worker 2: Send notification - simulates sending email/SMS/push
    let send_notification_worker = FnWorker::new("send_notification", |task: Task| async move {
        let user_id = task
            .get_input_string("user_id")
            .unwrap_or_else(|| "unknown".to_string());
        let message = task
            .get_input_string("message")
            .unwrap_or_else(|| "No message".to_string());

        info!("Sending notification to user_id={}: {}", user_id, message);

        // Simulate async notification service call
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut output = HashMap::new();
        output.insert("user_id".to_string(), serde_json::json!(user_id));
        output.insert("status".to_string(), serde_json::json!("sent"));

        info!("Notification sent to user_id={}", user_id);

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(100); // Very high concurrency for fast I/O tasks

    // ============================================================================
    // CPU-BOUND WORKER PATTERN
    // ============================================================================

    // Worker 3: Process image - simulates CPU-bound image processing
    let process_image_worker = FnWorker::new("process_image", |task: Task| async move {
        let image_url = task
            .get_input_string("image_url")
            .unwrap_or_else(|| "unknown.jpg".to_string());
        let filters: serde_json::Value = task
            .get_input("filters")
            .unwrap_or_else(|| serde_json::json!([]));

        info!(
            "Processing image: {} with filters: {:?}",
            image_url, filters
        );

        // Simulate CPU-intensive image processing
        // In a real app, you might use tokio::task::spawn_blocking for CPU work
        tokio::time::sleep(Duration::from_secs(2)).await;

        let output_url = format!("{}_processed", image_url);
        info!("Image processing complete: {}", output_url);

        let mut output = HashMap::new();
        output.insert("input_url".to_string(), serde_json::json!(image_url));
        output.insert("output_url".to_string(), serde_json::json!(output_url));
        output.insert("filters_applied".to_string(), filters);

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(4); // Lower concurrency for CPU-bound tasks

    // ============================================================================
    // LONG-RUNNING TASK PATTERN
    // ============================================================================

    // Worker 4: Long-running task with progress tracking
    let long_running_worker = FnWorker::new("long_running_task", |task: Task| async move {
        let job_id = task
            .get_input_string("job_id")
            .unwrap_or_else(|| "job_unknown".to_string());

        // Get poll count from task input (track progress across polls)
        let poll_count: i32 = task.get_input("poll_count").unwrap_or(0);

        info!("Processing job {}, poll {}/5", job_id, poll_count + 1);

        // Simulate work
        tokio::time::sleep(Duration::from_millis(500)).await;

        if poll_count < 4 {
            // Still processing - return InProgress to poll again
            let mut output = HashMap::new();
            output.insert("job_id".to_string(), serde_json::json!(job_id));
            output.insert("status".to_string(), serde_json::json!("processing"));
            output.insert("poll_count".to_string(), serde_json::json!(poll_count + 1));
            output.insert(
                "progress_percent".to_string(),
                serde_json::json!((poll_count + 1) * 20),
            );

            // Return InProgress with callback_after_seconds
            Ok(WorkerOutput::in_progress(1))
        } else {
            // Complete after 5 polls
            info!("Job {} completed", job_id);

            let mut output = HashMap::new();
            output.insert("job_id".to_string(), serde_json::json!(job_id));
            output.insert("status".to_string(), serde_json::json!("completed"));
            output.insert("result".to_string(), serde_json::json!("success"));
            output.insert("total_polls".to_string(), serde_json::json!(poll_count + 1));

            Ok(WorkerOutput::Completed(output))
        }
    })
    .with_thread_count(5);

    // ============================================================================
    // ERROR HANDLING PATTERN
    // ============================================================================

    // Worker 5: Demonstrates error handling
    let error_handling_worker = FnWorker::new("may_fail_task", |task: Task| async move {
        let should_fail: bool = task.get_input("should_fail").unwrap_or(false);

        info!("Task may_fail_task, should_fail={}", should_fail);

        if should_fail {
            // Return a failure result
            Ok(WorkerOutput::failed("Task deliberately failed for testing"))
        } else {
            let mut output = HashMap::new();
            output.insert("status".to_string(), serde_json::json!("success"));
            output.insert(
                "message".to_string(),
                serde_json::json!("Task completed successfully"),
            );

            Ok(WorkerOutput::Completed(output))
        }
    })
    .with_thread_count(10);

    // Add all workers to the handler
    handler.add_worker(fetch_user_worker);
    handler.add_worker(send_notification_worker);
    handler.add_worker(process_image_worker);
    handler.add_worker(long_running_worker);
    handler.add_worker(error_handling_worker);

    // Start the task handler
    info!("Starting task handler with workers...");
    handler.start().await?;

    println!("\n{}", "=".repeat(80));
    println!("Conductor Rust Worker Example - Async Workers");
    println!("{}", "=".repeat(80));
    println!("\nWorkers registered:");
    println!("  Async (I/O-bound):");
    println!("    - fetch_user_data: Fetch user data from API/DB (50 threads)");
    println!("    - send_notification: Send email/SMS/push notifications (100 threads)");
    println!("\n  CPU-bound:");
    println!("    - process_image: CPU-intensive image processing (4 threads)");
    println!("\n  Patterns:");
    println!("    - long_running_task: Polling-based long-running task (5 threads)");
    println!("    - may_fail_task: Demonstrates error handling (10 threads)");
    println!("\nPress Ctrl+C to stop");
    println!("{}\n", "=".repeat(80));

    // Execute a sample workflow to test the workers
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("worker_demo")
        .with_version(1)
        .with_input_value("user_id", "12345");

    match workflow_client.start_workflow(&request).await {
        Ok(workflow_id) => {
            info!("Started demo workflow: {}", workflow_id);
            info!("View at: {}", config.execution_url(&workflow_id));

            // Wait for workflow to complete
            tokio::time::sleep(Duration::from_secs(5)).await;

            let workflow = workflow_client.get_workflow(&workflow_id, false).await?;
            info!("Demo workflow status: {:?}", workflow.status);
        }
        Err(e) => {
            info!(
                "Failed to start demo workflow: {}. Workers are still running.",
                e
            );
        }
    }

    // Keep running until interrupted
    info!("Workers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    // Stop the handler
    handler.stop().await?;
    info!("Workers stopped. Goodbye!");

    Ok(())
}

async fn register_definitions(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // Register task definitions
    let task_defs = vec![
        TaskDef::new("fetch_user_data").with_description("Fetch user data from API/DB"),
        TaskDef::new("send_notification").with_description("Send notifications"),
        TaskDef::new("process_image").with_description("Process images"),
        TaskDef::new("long_running_task").with_description("Long-running task with progress"),
        TaskDef::new("may_fail_task").with_description("Task that may fail"),
    ];

    info!("Registering {} task definitions...", task_defs.len());
    metadata.register_task_defs(&task_defs).await?;

    // Create a sample workflow that uses the workers
    let workflow = WorkflowDef::new("worker_demo")
        .with_description("Demo workflow for worker example")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("fetch_user_data", "fetch_user_ref")
                .with_input_param("user_id", "${workflow.input.user_id}"),
        )
        .with_task(
            WorkflowTask::simple("send_notification", "send_notification_ref")
                .with_input_param("user_id", "${fetch_user_ref.output.user_id}")
                .with_input_param("message", "Welcome ${fetch_user_ref.output.name}!"),
        )
        .with_output_param("user", "${fetch_user_ref.output}")
        .with_output_param("notification", "${send_notification_ref.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
