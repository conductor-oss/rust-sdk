// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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

    // Build a workflow with multiple steps
    // Case 1 path: simple_task_1_case1 -> simple_task_2_case1
    let task1_case1 = WorkflowTask::wait("simple_task_ref1_case1_1");
    let task2_case1 = WorkflowTask::wait("simple_task_ref1_case1_2");
    let task3_case1 = WorkflowTask::wait("simple_task_ref2_case1_1");

    // Case 2 path
    let task1_case2 = WorkflowTask::wait("simple_task_ref1_case2_1");
    let task2_case2 = WorkflowTask::wait("simple_task_ref1_case2_2");

    // Switch based on input
    let switch_task = WorkflowTask::switch("switch_ref", "${workflow.input.case}")
        .with_switch_case("case1", vec![task1_case1, task2_case1, task3_case1])
        .with_switch_case("case2", vec![task1_case2, task2_case2]);

    let workflow_def = WorkflowDef::new("rerun_test")
        .with_description("Workflow to demonstrate rerun functionality")
        .with_version(1)
        .with_task(switch_task);

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);

    // Start workflow with case1
    let request = StartWorkflowRequest::new(&workflow_def.name)
        .with_version(1)
        .with_input_value("case", "case1");

    println!("\nStarting workflow with case=case1...");

    let workflow_run = workflow_client
        .execute_workflow_with_return_strategy(
            &request,
            None,
            Some("simple_task_ref1_case1_1"),
            10,
            None,
            None,
        )
        .await?;

    let workflow_id = &workflow_run.workflow_id;
    println!("Workflow started: {}", workflow_id);
    println!("Monitor at: {}/execution/{}", config.ui_host, workflow_id);

    // Complete first task
    println!("\nCompleting simple_task_ref1_case1_1...");
    let task_result = TaskResult {
        status: TaskResultStatus::Completed,
        ..Default::default()
    };

    let update = WorkflowStateUpdate {
        task_reference_name: Some("simple_task_ref1_case1_1".to_string()),
        task_result: Some(task_result.clone()),
        variables: HashMap::new(),
    };

    workflow_client
        .update_state(
            workflow_id,
            &update,
            Some(&["simple_task_ref1_case1_2".to_string()]),
            Some(5),
        )
        .await?;

    // Complete second task
    println!("Completing simple_task_ref1_case1_2...");
    let update2 = WorkflowStateUpdate {
        task_reference_name: Some("simple_task_ref1_case1_2".to_string()),
        task_result: Some(task_result.clone()),
        variables: HashMap::new(),
    };

    let workflow_run = workflow_client
        .update_state(
            workflow_id,
            &update2,
            Some(&["simple_task_ref2_case1_1".to_string()]),
            Some(5),
        )
        .await?;

    // Get the task we want to rerun from
    let rerun_task = workflow_run
        .tasks
        .iter()
        .find(|t| t.reference_task_name == "simple_task_ref1_case1_2")
        .expect("Task should exist");

    let rerun_from_task_id = &rerun_task.task_id;

    println!("\n--- Rerunning workflow from simple_task_ref1_case1_2 ---");
    println!("Task ID: {}", rerun_from_task_id);

    // Rerun the workflow from the second task
    workflow_client
        .rerun_workflow(
            workflow_id,
            rerun_from_task_id,
            None, // No task input override
            None, // No workflow input override
        )
        .await?;

    println!("Workflow rerun initiated!");

    // Check the status
    let workflow = workflow_client.get_workflow(workflow_id, true).await?;
    println!("\nWorkflow status after rerun: {:?}", workflow.status);

    // List the current tasks
    println!("\nCurrent tasks:");
    for task in &workflow.tasks {
        println!("  - {} ({:?})", task.reference_task_name, task.status);
    }

    println!("\nThe workflow has been rerun from simple_task_ref1_case1_2.");
    println!("The task simple_task_ref1_case1_2 and subsequent tasks are now pending.");

    Ok(())
}
