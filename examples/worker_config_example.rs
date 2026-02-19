// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
    schema::generate_schema,
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Input schema for the configured task
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ConfiguredTaskInput {
    /// Name of the user to process
    name: String,
    /// Priority level (1-10)
    priority: i32,
    /// Optional tags for the task
    #[serde(default)]
    tags: Vec<String>,
}

/// Output schema for the configured task
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ConfiguredTaskOutput {
    /// Processing result
    result: String,
    /// Timestamp of completion
    processed_at: String,
    /// Worker that processed this task
    worker_id: String,
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
    // Worker 1: Fully Configured Worker
    // ==============================
    // This worker demonstrates all configuration options
    let configured_worker = FnWorker::new("configured_task", |task: Task| async move {
        let name = task
            .get_input_string("name")
            .unwrap_or_else(|| "Unknown".to_string());
        let priority: i32 = task.get_input("priority").unwrap_or(5);

        info!(
            "[Configured Worker] Processing: name={}, priority={}",
            name, priority
        );

        // Simulate processing based on priority
        let delay = std::time::Duration::from_millis((100 * (11 - priority)) as u64);
        tokio::time::sleep(delay).await;

        Ok(WorkerOutput::completed_with_result(serde_json::json!({
            "result": format!("Processed {} with priority {}", name, priority),
            "processed_at": chrono::Utc::now().to_rfc3339(),
            "worker_id": hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string())
        })))
    })
    // Configuration options (can be overridden by environment variables)
    .with_thread_count(5) // Max concurrent executions
    .with_poll_interval_millis(200) // Poll every 200ms
    .with_domain("default") // Task routing domain
    .with_identity("rust-worker-configured") // Worker identity
    // JSON Schema for input/output validation
    .with_input_schema(generate_schema::<ConfiguredTaskInput>(true)) // strict mode
    .with_output_schema(generate_schema::<ConfiguredTaskOutput>(true));

    // ==============================
    // Worker 2: Worker with Type-Derived Schema
    // ==============================
    let schema_worker = FnWorker::new("schema_task", |task: Task| async move {
        let input: ConfiguredTaskInput = task.get_input("input").unwrap_or(ConfiguredTaskInput {
            name: "default".to_string(),
            priority: 5,
            tags: vec![],
        });

        info!(
            "[Schema Worker] Input: name={}, priority={}, tags={:?}",
            input.name, input.priority, input.tags
        );

        Ok(WorkerOutput::completed_with_result(serde_json::json!({
            "result": format!("Processed {}", input.name),
            "processed_at": chrono::Utc::now().to_rfc3339(),
            "worker_id": "schema-worker"
        })))
    })
    .with_thread_count(3)
    // Use helper method to generate schema from type
    .with_input_schema_from::<ConfiguredTaskInput>(true)
    .with_output_schema_from::<ConfiguredTaskOutput>(true);

    // ==============================
    // Worker 3: Minimal Configuration
    // ==============================
    // This worker uses mostly defaults, demonstrating that configuration is optional
    let minimal_worker = FnWorker::new("minimal_task", |task: Task| async move {
        let data = task.get_input_string("data").unwrap_or_default();
        info!("[Minimal Worker] Processing: {}", data);
        Ok(WorkerOutput::completed_with_result(format!(
            "Processed: {}",
            data
        )))
    });
    // Uses defaults: thread_count=1, poll_interval=100ms, no domain, no schema

    // ==============================
    // Worker 4: Domain-Specific Worker
    // ==============================
    // This worker only processes tasks from a specific domain
    let domain_worker = FnWorker::new("domain_task", |_task: Task| async move {
        info!("[Domain Worker] Processing task from domain");
        Ok(WorkerOutput::completed_with_result("domain task completed"))
    })
    .with_domain("special_domain") // Only processes tasks routed to this domain
    .with_thread_count(2);

    // Add all workers
    handler.add_worker(configured_worker);
    handler.add_worker(schema_worker);
    handler.add_worker(minimal_worker);
    handler.add_worker(domain_worker);

    // Start the handler (this will also register task definitions if configured)
    info!("\nStarting task handler...");
    info!("Configuration hierarchy:");
    info!("  1. Worker-specific env: CONDUCTOR_WORKER_<NAME>_<PROPERTY>");
    info!("  2. Global env: CONDUCTOR_WORKER_ALL_<PROPERTY>");
    info!("  3. Code defaults");

    handler.start().await?;

    // Print configuration summary
    println!("\n{}", "=".repeat(70));
    println!("Worker Configuration Example");
    println!("{}", "=".repeat(70));
    println!("\nRegistered Workers:");
    println!("  1. configured_task - Full configuration with JSON Schema");
    println!("     - thread_count: 5 (or CONDUCTOR_WORKER_CONFIGURED_TASK_THREAD_COUNT)");
    println!("     - poll_interval: 200ms");
    println!("     - domain: default");
    println!("     - input/output schema: enabled (strict mode)");
    println!("\n  2. schema_task - Type-derived JSON Schema");
    println!("     - Uses with_input_schema_from<T>()");
    println!("     - Automatic schema from Rust struct");
    println!("\n  3. minimal_task - Default configuration");
    println!("     - thread_count: 1 (default)");
    println!("     - poll_interval: 100ms (default)");
    println!("     - no domain, no schema");
    println!("\n  4. domain_task - Domain-specific routing");
    println!("     - domain: special_domain");
    println!("     - Only processes tasks with matching domain");
    println!("\nEnvironment Variable Examples:");
    println!("  CONDUCTOR_WORKER_ALL_THREAD_COUNT=10");
    println!("  CONDUCTOR_WORKER_CONFIGURED_TASK_THREAD_COUNT=20");
    println!("  CONDUCTOR_WORKER_CONFIGURED_TASK_PAUSED=true");
    println!("{}", "=".repeat(70));

    // Execute test workflow
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("worker_config_demo")
        .with_version(1)
        .with_input_value("name", "Test User")
        .with_input_value("priority", 8);

    info!("\nStarting test workflow...");
    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Workflow started: {}", workflow_id);
    info!("View at: {}", config.execution_url(&workflow_id));

    // Wait for completion
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let workflow = workflow_client.get_workflow(&workflow_id, false).await?;
    info!("Workflow status: {:?}", workflow.status);

    // Keep running
    info!("\nWorkers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("worker_config_demo")
        .with_description("Demonstrates worker configuration options")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("configured_task", "configured_ref")
                .with_input_param("name", "${workflow.input.name}")
                .with_input_param("priority", "${workflow.input.priority}"),
        )
        .with_task(
            WorkflowTask::simple("schema_task", "schema_ref").with_input_param(
                "input",
                serde_json::json!({
                    "name": "${workflow.input.name}",
                    "priority": "${workflow.input.priority}",
                    "tags": ["demo", "config"]
                }),
            ),
        )
        .with_task(
            WorkflowTask::simple("minimal_task", "minimal_ref")
                .with_input_param("data", "simple data"),
        )
        .with_output_param("configured_result", "${configured_ref.output}")
        .with_output_param("schema_result", "${schema_ref.output}")
        .with_output_param("minimal_result", "${minimal_ref.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
