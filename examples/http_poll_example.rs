// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::time::Duration;
use uuid::Uuid;

use conductor::{Configuration, OrkesClients, StartWorkflowRequest, WorkflowDef, WorkflowTask};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config)?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();

    // Create HTTP Poll task
    // This task will poll the API until the termination condition is met
    let http_poll = WorkflowTask::http_poll(
        "http_poll_ref",
        "https://orkes-api-tester.orkesconductor.com/api",
    )
    .with_polling_strategy("EXPONENTIAL_BACKOFF")
    .with_polling_interval(1000)
    // Termination condition: stop when randomInt < 10
    .with_termination_condition("(function(){ return $.output.response.body.randomInt < 10;})();");

    // Create workflow with unique name
    let workflow_name = format!("http_poll_example_{}", Uuid::new_v4());

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_description("HTTP Poll example - polls until condition met")
        .with_version(1)
        .with_task(http_poll)
        .with_output_param("result", "${http_poll_ref.output}");

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_name);
    println!("\nStarting HTTP Poll workflow...");
    println!("The workflow will poll until randomInt < 10\n");

    // Execute the workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);

    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(60))
        .await?;

    println!("Workflow status: {:?}", result.status);
    println!("\nFinal result:");
    println!("{}", serde_json::to_string_pretty(&result.output)?);

    // Cleanup - delete the workflow definition
    metadata_client
        .delete_workflow_def(&workflow_name, 1)
        .await?;

    println!("\nWorkflow definition cleaned up.");

    Ok(())
}
