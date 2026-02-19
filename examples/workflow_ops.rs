// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, TaskResult, WorkflowDef, WorkflowTask},
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

    // Load configuration
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config.clone())?;

    // Register the demo workflow
    register_workflow(&client).await?;

    let workflow_client = client.workflow_client();
    let task_client = client.task_client();

    // Start a new workflow
    let correlation_id = format!("rust_demo_{}", uuid::Uuid::new_v4());
    info!("Starting workflow with correlation_id: {}", correlation_id);

    let request = StartWorkflowRequest::new("workflow_ops_demo")
        .with_version(1)
        .with_correlation_id(&correlation_id);

    let workflow_id = workflow_client.start_workflow(&request).await?;
    info!("Started workflow with ID: {}", workflow_id);
    info!("Monitor at: {}", config.execution_url(&workflow_id));

    // Get workflow status
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status: {:?}", workflow.status);
    if let Some(last_task) = workflow.tasks.last() {
        info!(
            "Current task: {} (status: {:?})",
            last_task.reference_task_name, last_task.status
        );
    }

    // Wait for the timed wait task to complete
    info!("Waiting for 3 seconds for wait task to complete...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status: {:?}", workflow.status);
    if let Some(last_task) = workflow.tasks.last() {
        info!(
            "Current task: {} (status: {:?})",
            last_task.reference_task_name, last_task.status
        );
    }

    // Terminate the workflow
    info!("Terminating workflow...");
    workflow_client
        .terminate_workflow(&workflow_id, Some("Testing termination"), false)
        .await?;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status after terminate: {:?}", workflow.status);

    // Retry the workflow
    info!("Retrying workflow...");
    workflow_client.retry_workflow(&workflow_id, false).await?;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status after retry: {:?}", workflow.status);

    // Complete the wait_for_signal task manually
    if let Some(signal_task) = workflow
        .tasks
        .iter()
        .find(|t| t.reference_task_name == "wait_for_signal")
    {
        info!(
            "Completing wait_for_signal task manually (task_id: {})...",
            signal_task.task_id
        );

        let task_result = TaskResult::completed(&signal_task.task_id, &workflow_id)
            .with_worker_id("rust_manual")
            .with_output_value("message", "Signal received from Rust!");

        task_client.update_task(&task_result).await?;
        info!("Task completed successfully!");
    }

    // Wait for workflow to progress
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Terminate and restart
    info!("Terminating workflow to test restart...");
    workflow_client
        .terminate_workflow(&workflow_id, Some("Testing restart"), false)
        .await?;

    info!("Restarting workflow...");
    workflow_client
        .restart_workflow(&workflow_id, false)
        .await?;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status after restart: {:?}", workflow.status);

    // Test pause/resume
    info!("Pausing workflow...");
    workflow_client.pause_workflow(&workflow_id).await?;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!("Workflow status after pause: {:?}", workflow.status);

    // Wait and check that workflow is still paused
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!(
        "After 3 seconds, workflow status: {:?}, task count: {}",
        workflow.status,
        workflow.tasks.len()
    );

    // Resume the workflow
    info!("Resuming workflow...");
    workflow_client.resume_workflow(&workflow_id).await?;

    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    info!(
        "Workflow status after resume: {:?}, task count: {}",
        workflow.status,
        workflow.tasks.len()
    );

    // Search for workflows by correlation ID
    info!("Searching for workflows with correlation_id...");
    let query = format!("correlationId = \"{}\"", correlation_id);
    let search_results = workflow_client
        .search_workflows(Some(&query), None, 0, 100)
        .await?;
    info!(
        "Found {} workflow(s) with correlation_id: {}",
        search_results.total_hits, correlation_id
    );

    // Final cleanup - terminate the workflow
    info!("Final cleanup: terminating workflow...");
    workflow_client
        .terminate_workflow(&workflow_id, Some("Demo completed"), false)
        .await?;

    info!("Workflow operations demo completed!");
    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // Wait task with timeout
    let wait_2_sec = WorkflowTask::wait_duration("wait_for_2_sec", "2 seconds");

    // Wait task for signal (no timeout - async complete)
    let wait_for_signal = WorkflowTask::wait("wait_for_signal").async_complete();

    // HTTP task
    let http_task = WorkflowTask::http(
        "call_api",
        "https://orkes-api-tester.orkesconductor.com/api",
    );

    // Build the workflow
    let workflow = WorkflowDef::new("workflow_ops_demo")
        .with_description("Demonstrates workflow lifecycle operations")
        .with_version(1)
        .with_task(wait_2_sec)
        .with_task(wait_for_signal)
        .with_task(http_task);

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
