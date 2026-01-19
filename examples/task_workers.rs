//! Task Workers Example
//!
//! Demonstrates various worker patterns and configurations.
//!
//! What it shows:
//! - Simple workers with different configurations
//! - Worker with domain-based routing
//! - Batch polling configuration
//! - Task execution with context
//! - Error handling patterns
//!
//! Run with: cargo run --example task_workers
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

    // ==============================
    // Worker 1: Simple Echo Worker
    // ==============================
    let echo_worker = FnWorker::new("echo_task", |task: Task| async move {
        let message = task
            .get_input_string("message")
            .unwrap_or_else(|| "No message".to_string());

        info!("[Echo Worker] Received: {}", message);

        let mut output = HashMap::new();
        output.insert("echo".to_string(), serde_json::json!(message));
        output.insert(
            "timestamp".to_string(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(5)
    .with_poll_interval_millis(100);

    // ==============================
    // Worker 2: Transform Worker (data transformation)
    // ==============================
    let transform_worker = FnWorker::new("transform_task", |task: Task| async move {
        let data: serde_json::Value = task.get_input("data").unwrap_or(serde_json::json!({}));

        info!("[Transform Worker] Processing data: {:?}", data);

        // Simulate transformation
        let transformed = match data {
            serde_json::Value::Object(mut map) => {
                map.insert("transformed".to_string(), serde_json::json!(true));
                map.insert(
                    "processed_at".to_string(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                );
                serde_json::Value::Object(map)
            }
            _ => serde_json::json!({
                "original": data,
                "transformed": true
            }),
        };

        let mut output = HashMap::new();
        output.insert("result".to_string(), transformed);

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(10);

    // ==============================
    // Worker 3: Validation Worker (can fail)
    // ==============================
    let validate_worker = FnWorker::new("validate_task", |task: Task| async move {
        let required_field: Option<String> = task.get_input("required_field");

        info!(
            "[Validate Worker] Checking required_field: {:?}",
            required_field
        );

        match required_field {
            Some(value) if !value.is_empty() => {
                let mut output = HashMap::new();
                output.insert("valid".to_string(), serde_json::json!(true));
                output.insert("validated_value".to_string(), serde_json::json!(value));
                Ok(WorkerOutput::Completed(output))
            }
            _ => Ok(WorkerOutput::failed(
                "Validation failed: required_field is missing or empty",
            )),
        }
    })
    .with_thread_count(5);

    // ==============================
    // Worker 4: Slow Worker (simulates long-running task)
    // ==============================
    let slow_worker = FnWorker::new("slow_task", |task: Task| async move {
        let duration_ms: i64 = task.get_input("duration_ms").unwrap_or(1000);

        info!("[Slow Worker] Processing for {} ms", duration_ms);

        tokio::time::sleep(Duration::from_millis(duration_ms as u64)).await;

        let mut output = HashMap::new();
        output.insert(
            "message".to_string(),
            serde_json::json!(format!("Completed after {} ms", duration_ms)),
        );

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(2); // Low concurrency for slow tasks

    // ==============================
    // Worker 5: Batch Processor
    // ==============================
    let batch_worker = FnWorker::new("batch_task", |task: Task| async move {
        let items: Vec<serde_json::Value> = task.get_input("items").unwrap_or_default();

        info!("[Batch Worker] Processing {} items", items.len());

        let processed: Vec<serde_json::Value> = items
            .into_iter()
            .map(|item| {
                serde_json::json!({
                    "original": item,
                    "processed": true
                })
            })
            .collect();

        let mut output = HashMap::new();
        output.insert("processed_items".to_string(), serde_json::json!(processed));
        output.insert("count".to_string(), serde_json::json!(processed.len()));

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(3);

    // Add all workers
    handler.add_worker(echo_worker);
    handler.add_worker(transform_worker);
    handler.add_worker(validate_worker);
    handler.add_worker(slow_worker);
    handler.add_worker(batch_worker);

    // Start the handler
    info!("Starting task handler with 5 workers...");
    handler.start().await?;

    // Execute a test workflow
    let workflow_client = client.workflow_client();

    let test_data = serde_json::json!({
        "name": "Test",
        "value": 42
    });

    let request = StartWorkflowRequest::new("task_workers_demo")
        .with_version(1)
        .with_input_value("message", "Hello from Rust!")
        .with_input_value("data", test_data)
        .with_input_value("required_field", "present")
        .with_input_value("items", vec!["item1", "item2", "item3"]);

    info!("Starting demo workflow...");
    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Workflow started: {}", workflow_id);

    // Wait for completion
    tokio::time::sleep(Duration::from_secs(5)).await;

    let workflow = workflow_client.get_workflow(&workflow_id, false).await?;
    info!("Workflow status: {:?}", workflow.status);

    if workflow.is_successful() {
        info!("Workflow output: {:?}", workflow.output);
    }

    // Keep running
    info!("\nWorkers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    handler.stop().await?;
    info!("Workers stopped. Goodbye!");

    Ok(())
}

async fn register_definitions(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // Register task definitions
    let task_defs = vec![
        TaskDef::new("echo_task").with_description("Simple echo task"),
        TaskDef::new("transform_task").with_description("Data transformation task"),
        TaskDef::new("validate_task").with_description("Validation task"),
        TaskDef::new("slow_task").with_description("Slow processing task"),
        TaskDef::new("batch_task").with_description("Batch processing task"),
    ];

    info!("Registering {} task definitions...", task_defs.len());
    metadata.register_task_defs(&task_defs).await?;

    // Create workflow that uses all tasks
    let workflow = WorkflowDef::new("task_workers_demo")
        .with_description("Demo workflow for task workers example")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("echo_task", "echo_ref")
                .with_input_param("message", "${workflow.input.message}"),
        )
        .with_task(
            WorkflowTask::simple("transform_task", "transform_ref")
                .with_input_param("data", "${workflow.input.data}"),
        )
        .with_task(
            WorkflowTask::simple("validate_task", "validate_ref")
                .with_input_param("required_field", "${workflow.input.required_field}"),
        )
        .with_task(
            WorkflowTask::simple("batch_task", "batch_ref")
                .with_input_param("items", "${workflow.input.items}"),
        )
        .with_output_param("echo_result", "${echo_ref.output}")
        .with_output_param("transform_result", "${transform_ref.output}")
        .with_output_param("validate_result", "${validate_ref.output}")
        .with_output_param("batch_result", "${batch_ref.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
