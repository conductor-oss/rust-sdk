// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
    worker::{TaskContext, TaskHandler, WorkerOutput},
};
use conductor_macros::worker;
use tracing::info;

// ============================================================================
// Worker definitions using the #[worker] macro
// ============================================================================

/// Simple worker with primitive parameters
/// Parameters are automatically extracted from task input
#[worker(name = "greet", thread_count = 5, poll_interval = 100)]
async fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

/// Worker with multiple parameters
#[worker(name = "calculate", thread_count = 3)]
async fn calculate(a: i32, b: i32, operation: String) -> serde_json::Value {
    let result = match operation.as_str() {
        "add" => a + b,
        "subtract" => a - b,
        "multiply" => a * b,
        "divide" if b != 0 => a / b,
        _ => 0,
    };

    serde_json::json!({
        "a": a,
        "b": b,
        "operation": operation,
        "result": result
    })
}

/// Worker with Task parameter for full access
#[worker(name = "process_with_task")]
async fn process_with_task(task: Task) -> WorkerOutput {
    let data = task.get_input_string("data").unwrap_or_default();
    let count = task.poll_count;

    info!("Processing task with poll_count={}, data={}", count, data);

    WorkerOutput::completed_with_result(serde_json::json!({
        "processed": data,
        "poll_count": count
    }))
}

/// Worker with domain-specific routing
#[worker(name = "premium_task", domain = "premium", thread_count = 10)]
async fn premium_task(customer_id: String) -> String {
    format!("Premium processing for customer: {}", customer_id)
}

/// Worker that returns a Result
#[worker(name = "validate")]
async fn validate(value: String) -> std::result::Result<String, String> {
    if value.is_empty() {
        Err("Value cannot be empty".to_string())
    } else if value.len() > 100 {
        Err("Value too long".to_string())
    } else {
        Ok(format!("Valid: {}", value))
    }
}

/// Worker with TaskContext for accessing task metadata
/// This is the easiest way to get task_id, poll_count, etc.
#[worker(name = "batch_processor", thread_count = 5)]
async fn batch_processor(ctx: TaskContext, batch_size: i32) -> WorkerOutput {
    let offset = ctx.poll_count() * batch_size;

    info!(
        "Processing batch: task_id={}, poll_count={}, offset={}",
        ctx.task_id(),
        ctx.poll_count(),
        offset
    );

    if ctx.is_first_poll() {
        info!("First poll - initializing...");
    }

    // Simulate batch processing (in real code, process items at offset)
    let items_processed = batch_size;
    let is_complete = offset >= 100; // Simulate completion after 100 items

    if is_complete {
        WorkerOutput::completed_with_result(serde_json::json!({
            "total_processed": offset + items_processed,
            "status": "complete"
        }))
    } else {
        // More batches to process - call back in 5 seconds
        WorkerOutput::in_progress(5)
    }
}

/// Worker combining Task and TaskContext
/// Use this when you need both full task input access and convenient context methods
#[worker(name = "advanced_processor")]
async fn advanced_processor(task: Task, ctx: TaskContext) -> WorkerOutput {
    let data: String = task.get_input("data").unwrap_or_default();

    info!(
        "Advanced processing: task_id={}, workflow_id={}, retry={}, data={}",
        ctx.task_id(),
        ctx.workflow_instance_id(),
        ctx.retry_count(),
        data
    );

    if ctx.is_retry() {
        info!(
            "This is retry #{} - handling accordingly",
            ctx.retry_count()
        );
    }

    WorkerOutput::completed_with_result(serde_json::json!({
        "data": data,
        "task_id": ctx.task_id(),
        "poll_count": ctx.poll_count()
    }))
}

// ============================================================================
// Main function
// ============================================================================

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

    // Create task handler and add workers using the generated functions
    let mut handler = TaskHandler::new(config.clone())?;

    // Each #[worker] macro generates a `{fn_name}_worker()` function
    handler.add_worker(greet_worker());
    handler.add_worker(calculate_worker());
    handler.add_worker(process_with_task_worker());
    handler.add_worker(premium_task_worker());
    handler.add_worker(validate_worker());
    handler.add_worker(batch_processor_worker());
    handler.add_worker(advanced_processor_worker());

    // Start the handler
    info!("Starting task handler with macro-defined workers...");
    handler.start().await?;

    println!("\n{}", "=".repeat(70));
    println!("Worker Macro Example");
    println!("{}", "=".repeat(70));
    println!("\nWorkers defined using #[worker] macro:");
    println!("  1. greet              - Simple string return");
    println!("  2. calculate          - Multiple params, JSON return");
    println!("  3. process_with_task  - Full Task access");
    println!("  4. premium_task       - Domain-specific routing");
    println!("  5. validate           - Result<T, E> return type");
    println!("  6. batch_processor    - TaskContext for metadata access");
    println!("  7. advanced_processor - Combined Task + TaskContext");
    println!("\nThe macro generates `_worker()` functions:");
    println!("  greet_worker(), batch_processor_worker(), etc.");
    println!("{}", "=".repeat(70));

    // Execute test workflows
    let workflow_client = client.workflow_client();

    // Test greet worker
    info!("\nTesting greet worker...");
    let request = StartWorkflowRequest::new("macro_demo")
        .with_version(1)
        .with_input_value("task_type", "greet")
        .with_input_value("name", "Rust Macro User");
    let wf_id = workflow_client.start_workflow(&request).await?;
    info!("Started workflow: {}", wf_id);

    // Test calculate worker
    info!("\nTesting calculate worker...");
    let request = StartWorkflowRequest::new("macro_demo")
        .with_version(1)
        .with_input_value("task_type", "calculate")
        .with_input_value("a", 10)
        .with_input_value("b", 5)
        .with_input_value("operation", "multiply");
    let wf_id = workflow_client.start_workflow(&request).await?;
    info!("Started workflow: {}", wf_id);

    // Test validate worker
    info!("\nTesting validate worker...");
    let request = StartWorkflowRequest::new("macro_demo")
        .with_version(1)
        .with_input_value("task_type", "validate")
        .with_input_value("value", "test value");
    let wf_id = workflow_client.start_workflow(&request).await?;
    info!("Started workflow: {}", wf_id);

    // Wait for processing
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Keep running
    info!("\nWorkers are running. Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("macro_demo")
        .with_description("Demonstrates #[worker] macro")
        .with_version(1)
        .with_task(
            WorkflowTask::switch("select_task", "$.task_type")
                .with_input_param("task_type", "${workflow.input.task_type}")
                .with_switch_case(
                    "greet",
                    vec![WorkflowTask::simple("greet", "greet_ref")
                        .with_input_param("name", "${workflow.input.name}")],
                )
                .with_switch_case(
                    "calculate",
                    vec![WorkflowTask::simple("calculate", "calc_ref")
                        .with_input_param("a", "${workflow.input.a}")
                        .with_input_param("b", "${workflow.input.b}")
                        .with_input_param("operation", "${workflow.input.operation}")],
                )
                .with_switch_case(
                    "validate",
                    vec![WorkflowTask::simple("validate", "validate_ref")
                        .with_input_param("value", "${workflow.input.value}")],
                )
                .with_default_case(vec![WorkflowTask::simple(
                    "process_with_task",
                    "process_ref",
                )
                .with_input_param("data", "${workflow.input.data}")]),
        )
        .with_output_param("result", "${select_task.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
