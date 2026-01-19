//! Async Workers Example
//!
//! This example demonstrates:
//! - Multiple async workers
//! - Concurrent task execution
//! - Prometheus metrics
//! - Long-running tasks with TaskInProgress
//!
//! Run with: cargo run --example async_workers

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    metrics::MetricsSettings,
    models::{StartWorkflowRequest, Task, TaskInProgress, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
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

    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    let client = ConductorClient::new(config.clone())?;

    // Register workflow
    register_workflow(&client).await?;

    // Create workers
    let workers = create_workers();

    // Create task handler with metrics
    let mut handler = TaskHandler::new(config.clone())?;

    for worker in workers {
        handler.add_worker(worker);
    }

    // Enable Prometheus metrics on port 9090
    handler.enable_metrics(MetricsSettings::new().with_http_port(9090));

    info!("Starting workers...");
    handler.start().await?;

    info!("Metrics available at http://localhost:9090/metrics");

    // Execute the workflow
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("async_demo")
        .with_version(1)
        .with_input_value("user_id", "user_123")
        .with_input_value("url", "https://api.example.com/data");

    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Workflow started: {}", workflow_id);
    info!("View execution at: {}", config.execution_url(&workflow_id));

    // Wait for workflow or Ctrl+C
    info!("Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await?;

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

fn create_workers() -> Vec<FnWorker> {
    vec![
        // Fast async worker (simulates API call)
        FnWorker::new("fetch_user_data", |task: Task| async move {
            let user_id = task
                .get_input_string("user_id")
                .unwrap_or_else(|| "unknown".to_string());

            info!("Fetching data for user: {}", user_id);

            // Simulate async API call
            tokio::time::sleep(Duration::from_millis(100)).await;

            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "user_id": user_id,
                "name": "John Doe",
                "email": "john@example.com"
            })))
        })
        .with_thread_count(10)
        .with_poll_interval_millis(100),
        // Another async worker (simulates external API)
        FnWorker::new("fetch_external_api", |task: Task| async move {
            let url = task
                .get_input_string("url")
                .unwrap_or_else(|| "https://example.com".to_string());

            info!("Fetching external API: {}", url);

            // Simulate API call
            tokio::time::sleep(Duration::from_millis(200)).await;

            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "url": url,
                "status": 200,
                "data": {"key": "value"}
            })))
        })
        .with_thread_count(20)
        .with_poll_interval_millis(100),
        // Long-running worker demonstrating TaskInProgress
        FnWorker::new("process_batch", |task: Task| async move {
            let batch_id = task
                .get_input_string("batch_id")
                .unwrap_or_else(|| "batch_0".to_string());

            let poll_count = task.poll_count;

            info!("Processing batch: {}, poll count: {}", batch_id, poll_count);

            // Simulate batch processing over multiple polls
            if poll_count < 3 {
                // Still processing - return IN_PROGRESS
                tokio::time::sleep(Duration::from_millis(500)).await;

                Ok(WorkerOutput::InProgress(
                    TaskInProgress::new(5) // Callback after 5 seconds
                        .with_output_value("progress", (poll_count + 1) * 33)
                        .with_output_value("status", "processing"),
                ))
            } else {
                // Done processing
                Ok(WorkerOutput::completed_with_result(serde_json::json!({
                    "batch_id": batch_id,
                    "processed_items": 100,
                    "status": "completed"
                })))
            }
        })
        .with_thread_count(5)
        .with_poll_interval_millis(500),
        // CPU-bound worker (lower concurrency)
        FnWorker::new("compute_hash", |task: Task| async move {
            let data = task
                .get_input_string("data")
                .unwrap_or_else(|| "default".to_string());

            info!("Computing hash for data: {}", &data[..data.len().min(20)]);

            // Simulate CPU-bound work
            tokio::task::spawn_blocking(move || {
                // Simulate hash computation
                std::thread::sleep(Duration::from_millis(50));
                format!("hash_{:x}", data.len())
            })
            .await
            .map(WorkerOutput::completed_with_result)
            .map_err(|e| conductor::error::ConductorError::internal(e.to_string()))
        })
        .with_thread_count(4)
        .with_poll_interval_millis(200),
    ]
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("async_demo")
        .with_description("Demo workflow for async workers")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("fetch_user_data", "fetch_user_ref")
                .with_input_param("user_id", "${workflow.input.user_id}"),
        )
        .with_task(
            WorkflowTask::simple("fetch_external_api", "fetch_api_ref")
                .with_input_param("url", "${workflow.input.url}"),
        )
        .with_task(
            WorkflowTask::simple("compute_hash", "compute_hash_ref")
                .with_input_param("data", "${fetch_user_ref.output.result.email}"),
        )
        .with_output_param("user", "${fetch_user_ref.output.result}")
        .with_output_param("api_data", "${fetch_api_ref.output.result}")
        .with_output_param("hash", "${compute_hash_ref.output.result}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
