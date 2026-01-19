//! Dynamic Workflow Example
//!
//! This example demonstrates creating and executing workflows dynamically
//! without pre-registering them.
//!
//! Run with: cargo run --example dynamic_workflow

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
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

    // Create workers for our dynamic workflow
    let get_user_email = FnWorker::new("get_user_email", |task: Task| async move {
        let user_id = task
            .get_input_string("userid")
            .unwrap_or_else(|| "unknown".to_string());

        info!("Getting email for user: {}", user_id);

        Ok(WorkerOutput::completed_with_result(format!(
            "{}@example.com",
            user_id
        )))
    })
    .with_thread_count(5);

    let send_email = FnWorker::new("send_email", |task: Task| async move {
        let email = task
            .get_input_string("email")
            .unwrap_or_else(|| "unknown@example.com".to_string());
        let subject = task
            .get_input_string("subject")
            .unwrap_or_else(|| "No Subject".to_string());
        let _body = task
            .get_input_string("body")
            .unwrap_or_else(|| "No Body".to_string());

        info!("Sending email to {} with subject: {}", email, subject);

        // Simulate sending email
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(WorkerOutput::completed_with_result(serde_json::json!({
            "sent": true,
            "email": email,
            "subject": subject
        })))
    })
    .with_thread_count(5);

    // Start task handler with workers
    let mut handler = TaskHandler::new(config.clone())?;
    handler.add_worker(get_user_email);
    handler.add_worker(send_email);
    handler.start().await?;

    // Create a dynamic workflow definition
    let workflow_def = WorkflowDef::new("dynamic_workflow")
        .with_version(1)
        .with_description("Dynamically created workflow")
        .with_task(
            WorkflowTask::simple("get_user_email", "get_user_email_ref")
                .with_input_param("userid", "${workflow.input.userid}"),
        )
        .with_task(
            WorkflowTask::simple("send_email", "send_email_ref")
                .with_input_param("email", "${get_user_email_ref.output.result}")
                .with_input_param("subject", "Hello from Conductor Rust SDK")
                .with_input_param("body", "This is a test email from a dynamic workflow"),
        )
        .with_output_param("email", "${get_user_email_ref.output.result}")
        .with_output_param("sent", "${send_email_ref.output.result.sent}");

    // Execute the dynamic workflow
    info!("Starting dynamic workflow...");

    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("dynamic_workflow")
        .with_version(1)
        .with_input_value("userid", "user_a")
        .with_workflow_def(workflow_def);

    // Execute synchronously and wait for result
    let workflow_run = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await?;

    info!("Workflow status: {:?}", workflow_run.status);
    info!("Workflow output: {:?}", workflow_run.output);

    if let Some(email) = workflow_run.output.get("email") {
        info!("Email result: {}", email);
    }

    // Stop the handler
    handler.stop().await?;

    info!("Done!");
    Ok(())
}
