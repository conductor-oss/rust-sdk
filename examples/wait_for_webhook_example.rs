//! Wait for Webhook Example
//!
//! This example demonstrates how to use Wait for Webhook tasks that pause
//! workflow execution until an external webhook is received.
//! It mirrors the Python SDK's orkes/wait_for_webhook.py example.
//!
//! Prerequisites:
//! - Conductor server running
//! - CONDUCTOR_SERVER_URL environment variable set
//! - A webhook configured in Conductor UI to dispatch to this workflow

use std::collections::HashMap;
use std::time::Duration;

use conductor::worker::{FnWorker, WorkerOutput};
use conductor::{
    Configuration, OrkesClients, StartWorkflowRequest, TaskHandler, WorkflowDef, WorkflowTask,
};

/// Worker to get user's email based on user ID
async fn get_user_email(task: conductor::Task) -> conductor::Result<WorkerOutput> {
    let userid = task.get_input_string("userid").unwrap_or_default();
    let email = format!("{}@example.com", userid);
    Ok(WorkerOutput::completed_with_result(serde_json::json!({
        "result": email
    })))
}

/// Worker to send email (simulated)
async fn send_email(task: conductor::Task) -> conductor::Result<WorkerOutput> {
    let email = task.get_input_string("email").unwrap_or_default();
    let subject = task.get_input_string("subject").unwrap_or_default();
    let body = task.get_input_string("body").unwrap_or_default();

    println!(
        "Sending email to {} with subject '{}' and body '{}'",
        email, subject, body
    );

    Ok(WorkerOutput::complete())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config.clone())?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();

    // Start workers
    let mut handler = TaskHandler::new(config.clone())?;
    handler.add_worker(FnWorker::new("get_user_email", get_user_email));
    handler.add_worker(FnWorker::new("send_email", send_email));
    handler.start().await?;

    // Build workflow:
    // 1. Get user email
    // 2. Send welcome email
    // 3. Wait for webhook confirmation

    let get_email = WorkflowTask::simple("get_user_email", "get_user_email_ref")
        .with_input_param("userid", "${workflow.input.userid}");

    let send_email_task = WorkflowTask::simple("send_email", "send_email_ref")
        .with_input_param("email", "${get_user_email_ref.output.result}")
        .with_input_param("subject", "Hello from Orkes")
        .with_input_param("body", "Welcome! Please confirm your account.");

    // Wait for webhook with matching criteria
    // The webhook must have type="customer" and id matching the userid
    let mut matches = HashMap::new();
    matches.insert("$['type']".to_string(), "customer".to_string());
    matches.insert(
        "$['id']".to_string(),
        "${workflow.input.userid}".to_string(),
    );

    let wait_webhook = WorkflowTask::wait_for_webhook("wait_ref").with_matches(matches);

    let workflow_def = WorkflowDef::new("wait_for_webhook_example")
        .with_description("Workflow that waits for external webhook confirmation")
        .with_version(1)
        .with_task(get_email)
        .with_task(send_email_task)
        .with_task(wait_webhook)
        .with_output_param("confirmed", "${wait_ref.output}");

    // Register workflow - webhook workflows MUST be registered before use
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);
    println!("\nIMPORTANT: Create a webhook in Conductor UI that dispatches to this workflow.");
    println!("See: https://orkes.io/content/reference-docs/system-tasks/wait-for-webhook\n");

    // Start workflow
    let request = StartWorkflowRequest::new(&workflow_def.name)
        .with_version(1)
        .with_input_value("userid", "user_a");

    let workflow_id = workflow_client.start_workflow(&request).await?;

    println!("Workflow started: {}", workflow_id);
    println!("\nThe workflow is now waiting for a webhook.");
    println!("\nTo complete the workflow, send a POST request to your webhook URL:");
    println!(
        r#"
curl --location 'http://localhost:8080/webhook/YOUR_WEBHOOK_ID' \
    --header 'Content-Type: application/json' \
    --data '{{
        "id": "user_a",
        "type": "customer"
    }}'
"#
    );

    // In a real application, you would wait for the webhook to be received
    // For this example, we'll just wait a bit then check the status
    println!("\nWaiting 5 seconds...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check workflow status
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    println!("Current workflow status: {:?}", workflow.status);

    if workflow.is_running() {
        println!("\nWorkflow is still waiting for webhook. Send the webhook to complete it.");
    }

    // Cleanup
    handler.stop().await?;

    Ok(())
}
