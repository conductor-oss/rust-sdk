//! Sync State Update Example
//!
//! This example demonstrates how to update workflow state synchronously,
//! including completing wait tasks and setting workflow variables.
//! It mirrors the Python SDK's orkes/sync_updates.py example.
//!
//! Prerequisites:
//! - Conductor server running
//! - CONDUCTOR_SERVER_URL environment variable set

use std::collections::HashMap;

use conductor::client::WorkflowStateUpdate;
use conductor::{
    Configuration, OrkesClients, StartWorkflowRequest, TaskResult, TaskResultStatus, WorkflowDef,
    WorkflowTask,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config.clone())?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();

    // Build workflow with wait tasks and a switch based on variable
    let http_task = WorkflowTask::http(
        "http_ref",
        "https://orkes-api-tester.orkesconductor.com/api",
    );

    let wait_task = WorkflowTask::wait("wait_task_ref");

    let wait_case_1 = WorkflowTask::wait("wait_task_ref_1");
    let wait_case_2 = WorkflowTask::wait("wait_task_ref_2");

    // Switch based on workflow variable
    let switch_task = WorkflowTask::switch("switch_ref", "${workflow.variables.case}")
        .with_switch_case("case1", vec![wait_case_1])
        .with_switch_case("case2", vec![wait_case_2]);

    let workflow_def = WorkflowDef::new("sync_task_variable_updates")
        .with_description("Demonstrates synchronous state updates")
        .with_version(1)
        .with_task(http_task)
        .with_task(wait_task)
        .with_task(switch_task);

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);

    // Execute workflow and wait until the first wait task
    let request = StartWorkflowRequest::new(&workflow_def.name).with_version(1);

    println!("\nStarting workflow and waiting for wait_task_ref...");

    // Use execute_workflow_with_return_strategy for better control
    let workflow_run = workflow_client
        .execute_workflow_with_return_strategy(
            &request,
            None,
            Some("wait_task_ref"),
            10,
            None,
            None,
        )
        .await?;

    let workflow_id = &workflow_run.workflow_id;
    println!("Workflow started: {}", workflow_id);
    println!(
        "See execution at: {}/execution/{}",
        config.ui_host, workflow_id
    );

    // Complete the first wait task and set a variable to control the switch
    println!("\nCompleting wait_task_ref and setting variable 'case' to 'case1'...");

    let task_result = TaskResult {
        status: TaskResultStatus::Completed,
        ..Default::default()
    };

    let mut variables = HashMap::new();
    variables.insert("case".to_string(), serde_json::json!("case1"));

    let state_update = WorkflowStateUpdate {
        task_reference_name: Some("wait_task_ref".to_string()),
        task_result: Some(task_result.clone()),
        variables,
    };

    let workflow_run = workflow_client
        .update_state(
            workflow_id,
            &state_update,
            Some(&["wait_task_ref_1".to_string(), "wait_task_ref_2".to_string()]),
            Some(5),
        )
        .await?;

    println!(
        "Workflow status after first update: {:?}",
        workflow_run.status
    );

    // Get the last task reference name (should be wait_task_ref_1 since we set case=case1)
    let last_task_ref = workflow_run
        .tasks
        .last()
        .map(|t| t.reference_task_name.as_str())
        .unwrap_or("unknown");

    println!("Last task: {} (expected wait_task_ref_1)", last_task_ref);

    // Complete the final wait task
    println!("\nCompleting {}...", last_task_ref);

    let final_update = WorkflowStateUpdate {
        task_reference_name: Some(last_task_ref.to_string()),
        task_result: Some(task_result),
        variables: HashMap::new(),
    };

    let final_run = workflow_client
        .update_state(workflow_id, &final_update, None, Some(5))
        .await?;

    println!("Final workflow status: {:?}", final_run.status);

    Ok(())
}
