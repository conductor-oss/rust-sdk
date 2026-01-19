//! Schedule Journey Example
//!
//! Demonstrates workflow scheduling with cron expressions.
//!
//! What it shows:
//! - Creating scheduled workflows
//! - Cron expressions for scheduling
//! - Pause/resume schedules
//! - Querying schedule executions
//! - Schedule tags
//!
//! Run with: cargo run --example schedule_journey
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{MetadataTag, SaveScheduleRequest, WorkflowDef, WorkflowTask},
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

    // Register a simple workflow for scheduling
    register_workflow(&client).await?;

    let scheduler = client.scheduler_client();

    // ==============================
    // Create a Schedule
    // ==============================
    info!("\n=== Creating a Schedule ===");

    let schedule_name = format!("rust_schedule_demo_{}", uuid::Uuid::new_v4());

    // Create the schedule (runs every minute)
    let schedule_request =
        SaveScheduleRequest::new(&schedule_name, "0 * * * * ?", "scheduled_workflow").paused(true); // Start paused to avoid actual executions

    scheduler.save_schedule(&schedule_request).await?;
    info!("Schedule '{}' created (paused)", schedule_name);

    // ==============================
    // Get Schedule Details
    // ==============================
    info!("\n=== Getting Schedule Details ===");

    let schedule = scheduler.get_schedule(&schedule_name).await?;
    info!("Schedule: {}", schedule.name);
    info!("Cron: {}", schedule.cron_expression);
    info!("Paused: {}", schedule.paused);

    // ==============================
    // Get Next Execution Times
    // ==============================
    info!("\n=== Getting Next Execution Times ===");

    let next_times = scheduler
        .get_next_few_schedule_execution_times("0 * * * * ?", None, None, Some(5))
        .await?;

    info!("Next 5 scheduled execution times:");
    for (i, timestamp) in next_times.iter().enumerate() {
        let datetime = chrono::DateTime::from_timestamp_millis(*timestamp)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "Invalid".to_string());
        info!("  {}: {} ({})", i + 1, datetime, timestamp);
    }

    // ==============================
    // Add Schedule Tags
    // ==============================
    info!("\n=== Adding Schedule Tags ===");

    let tags = vec![
        MetadataTag::with_value("environment", "demo"),
        MetadataTag::with_value("owner", "rust-sdk"),
    ];

    scheduler.set_scheduler_tags(&tags, &schedule_name).await?;
    info!("Tags added to schedule");

    // Get tags back
    let retrieved_tags = scheduler.get_scheduler_tags(&schedule_name).await?;
    info!("Retrieved tags:");
    for tag in &retrieved_tags {
        info!("  {} = {:?}", tag.key, tag.value);
    }

    // ==============================
    // Resume Schedule (briefly)
    // ==============================
    info!("\n=== Testing Resume/Pause ===");

    scheduler.resume_schedule(&schedule_name).await?;
    info!("Schedule resumed");

    // Check status
    let schedule = scheduler.get_schedule(&schedule_name).await?;
    info!("Schedule paused status: {}", schedule.paused);

    // Pause again
    scheduler.pause_schedule(&schedule_name).await?;
    info!("Schedule paused again");

    // ==============================
    // List All Schedules
    // ==============================
    info!("\n=== Listing All Schedules ===");

    let all_schedules = scheduler.get_all_schedules(None).await?;
    info!("Total schedules: {}", all_schedules.len());

    // Filter by workflow name
    let workflow_schedules = scheduler
        .get_all_schedules(Some("scheduled_workflow"))
        .await?;
    info!(
        "Schedules for 'scheduled_workflow': {}",
        workflow_schedules.len()
    );

    // ==============================
    // Search Schedule Executions
    // ==============================
    info!("\n=== Searching Schedule Executions ===");

    let executions = scheduler
        .search_schedule_executions(Some(0), Some(10), None, None, None)
        .await?;
    info!("Found {} schedule executions", executions.results.len());

    // ==============================
    // Cleanup - Delete Schedule
    // ==============================
    info!("\n=== Cleanup ===");

    scheduler.delete_schedule(&schedule_name).await?;
    info!("Schedule '{}' deleted", schedule_name);

    info!("\nSchedule journey example completed!");
    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // Simple HTTP task for scheduled execution
    let http_task = WorkflowTask::http(
        "scheduled_http_call",
        "https://orkes-api-tester.orkesconductor.com/api",
    );

    let workflow = WorkflowDef::new("scheduled_workflow")
        .with_description("Workflow for demonstrating scheduling")
        .with_version(1)
        .with_task(http_task)
        .with_output_param("response", "${scheduled_http_call.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
