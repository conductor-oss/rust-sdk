// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
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

    // Load configuration from environment
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config.clone())?;

    // Register the workflow definition
    register_workflow(&client).await?;

    // Create the greet worker
    let greet_worker = FnWorker::new("greet", |task: Task| async move {
        let name = task
            .get_input_string("name")
            .unwrap_or_else(|| "World".to_string());

        info!("Greeting: {}", name);

        let mut output = std::collections::HashMap::new();
        output.insert(
            "result".to_string(),
            serde_json::json!(format!("Hello, {}!", name)),
        );

        Ok(WorkerOutput::Completed(output))
    })
    .with_thread_count(5);

    // Create and start the task handler
    let mut handler = TaskHandler::new(config.clone())?;
    handler.add_worker(greet_worker);
    handler.start().await?;

    // Execute the workflow
    info!("Starting workflow execution...");
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("greetings")
        .with_version(1)
        .with_input_value("name", "Conductor Rust SDK");

    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Workflow started: {}", workflow_id);
    info!("View execution at: {}", config.execution_url(&workflow_id));

    // Wait for workflow to complete
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // Get workflow result
    let workflow = workflow_client.get_workflow(&workflow_id, false).await?;
    info!("Workflow status: {:?}", workflow.status);

    if workflow.is_successful() {
        if let Some(result) = workflow.output.get("result") {
            info!("Workflow result: {}", result);
        }
    }

    // Stop the handler
    handler.stop().await?;

    info!("Done!");
    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // Define the greetings workflow
    let workflow = WorkflowDef::new("greetings")
        .with_description("Sample greetings workflow")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("greet", "greet_ref")
                .with_input_param("name", "${workflow.input.name}"),
        )
        .with_output_param("result", "${greet_ref.output.result}");

    // Register the workflow
    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
