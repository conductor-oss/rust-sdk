//! Metadata Journey Example
//!
//! Demonstrates workflow and task definition management.
//!
//! What it shows:
//! - Creating and updating workflow definitions
//! - Creating and updating task definitions
//! - Managing workflow/task tags (via OrkesMetadataClient)
//! - Querying definitions
//! - Version management
//!
//! Run with: cargo run --example metadata_journey
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{MetadataTag, RetryLogic, TaskDef, TimeoutPolicy, WorkflowDef, WorkflowTask},
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
    let client = ConductorClient::new(config)?;
    let metadata = client.metadata_client();
    // OrkesMetadataClient extends MetadataClient with tagging APIs
    let orkes_metadata = client.orkes_metadata_client();

    // ==============================
    // Task Definitions
    // ==============================
    info!("\n=== Task Definition Management ===");

    // Create a task definition with full configuration
    let task_name = format!("rust_demo_task_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let task_def = TaskDef::new(&task_name)
        .with_description("Demo task created by Rust SDK")
        .with_retry(3, RetryLogic::ExponentialBackoff, 5)
        .with_timeout(300, TimeoutPolicy::Retry)
        .with_rate_limit(100, 60)
        .with_concurrent_limit(10);

    info!("Registering task definition: {}", task_name);
    metadata.register_task_def(&task_def).await?;

    // Retrieve the task definition
    let retrieved_task = metadata.get_task_def(&task_name).await?;
    info!("Retrieved task: {}", retrieved_task.name);
    info!("  Retry count: {:?}", retrieved_task.retry_count);
    info!("  Timeout: {:?} seconds", retrieved_task.timeout_seconds);

    // Update the task definition
    let updated_task = TaskDef::new(&task_name)
        .with_description("Updated demo task")
        .with_retry(5, RetryLogic::Fixed, 10)
        .with_timeout(600, TimeoutPolicy::AlertOnly);

    info!("Updating task definition...");
    metadata.update_task_def(&updated_task).await?;

    let retrieved_updated = metadata.get_task_def(&task_name).await?;
    info!("Updated retry count: {:?}", retrieved_updated.retry_count);

    // Add tags to task (using OrkesMetadataClient)
    info!("\nAdding tags to task...");
    let task_tag = MetadataTag::with_value("team", "platform");
    orkes_metadata.add_task_tag(&task_name, &task_tag).await?;

    let task_tags = orkes_metadata.get_task_tags(&task_name).await?;
    info!("Task tags: {:?}", task_tags);

    // ==============================
    // Workflow Definitions
    // ==============================
    info!("\n=== Workflow Definition Management ===");

    let workflow_name = format!(
        "rust_demo_workflow_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    // Create a workflow with multiple versions
    let workflow_v1 = WorkflowDef::new(&workflow_name)
        .with_description("Demo workflow v1")
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref_1"))
        .with_output_param("result", "${task_ref_1.output}");

    info!("Registering workflow v1: {}", workflow_name);
    metadata.register_workflow_def(&workflow_v1).await?;

    // Create version 2
    let workflow_v2 = WorkflowDef::new(&workflow_name)
        .with_description("Demo workflow v2 with HTTP task")
        .with_version(2)
        .with_task(WorkflowTask::simple(&task_name, "task_ref_1"))
        .with_task(WorkflowTask::http(
            "http_task",
            "https://orkes-api-tester.orkesconductor.com/api",
        ))
        .with_output_param("task_result", "${task_ref_1.output}")
        .with_output_param("http_result", "${http_task.output}");

    info!("Registering workflow v2...");
    metadata.update_workflow_def(&workflow_v2).await?;

    // Get specific version
    let wf_v1 = metadata.get_workflow_def(&workflow_name, Some(1)).await?;
    info!("Workflow v1 tasks: {}", wf_v1.tasks.len());

    let wf_v2 = metadata.get_workflow_def(&workflow_name, Some(2)).await?;
    info!("Workflow v2 tasks: {}", wf_v2.tasks.len());

    // Get all versions
    let all_versions = metadata
        .get_all_workflow_def_versions(&workflow_name)
        .await?;
    info!("Total versions: {}", all_versions.len());

    // Add tags to workflow (using OrkesMetadataClient)
    info!("\nAdding tags to workflow...");
    let workflow_tags = vec![
        MetadataTag::with_value("environment", "demo"),
        MetadataTag::with_value("language", "rust"),
    ];
    orkes_metadata
        .set_workflow_tags(&workflow_name, &workflow_tags)
        .await?;

    let retrieved_workflow_tags = orkes_metadata.get_workflow_tags(&workflow_name).await?;
    info!("Workflow tags:");
    for tag in &retrieved_workflow_tags {
        info!("  {} = {:?}", tag.key, tag.value);
    }

    // ==============================
    // Query Operations
    // ==============================
    info!("\n=== Query Operations ===");

    // List all task definitions
    let all_tasks = metadata.get_all_task_defs().await?;
    info!("Total task definitions: {}", all_tasks.len());

    // List all workflow definitions
    let all_workflows = metadata.get_all_workflow_defs().await?;
    info!("Total workflow definitions: {}", all_workflows.len());

    // Check existence
    let task_exists = metadata.task_def_exists(&task_name).await?;
    info!("Task '{}' exists: {}", task_name, task_exists);

    let workflow_exists = metadata.workflow_def_exists(&workflow_name, None).await?;
    info!("Workflow '{}' exists: {}", workflow_name, workflow_exists);

    let nonexistent = metadata.task_def_exists("nonexistent_task").await?;
    info!("Nonexistent task exists: {}", nonexistent);

    // ==============================
    // Cleanup
    // ==============================
    info!("\n=== Cleanup ===");

    // Delete tags first (using OrkesMetadataClient)
    orkes_metadata
        .delete_workflow_tag(&workflow_name, &workflow_tags[0])
        .await?;
    info!("Deleted workflow tag");

    orkes_metadata.delete_task_tag(&task_name, &task_tag).await?;
    info!("Deleted task tag");

    // Delete workflow versions (base MetadataClient via Deref)
    orkes_metadata.delete_workflow_def(&workflow_name, 1).await?;
    info!("Deleted workflow v1");

    orkes_metadata.delete_workflow_def(&workflow_name, 2).await?;
    info!("Deleted workflow v2");

    // Delete task
    orkes_metadata.delete_task_def(&task_name).await?;
    info!("Deleted task definition");

    info!("\nMetadata journey example completed!");
    Ok(())
}
