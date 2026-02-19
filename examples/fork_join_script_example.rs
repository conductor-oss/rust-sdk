// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{Configuration, OrkesClients, StartWorkflowRequest, WorkflowDef, WorkflowTask};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config.clone())?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();

    // Create multiple parallel HTTP tasks
    // These will call an endpoint that may return 404, demonstrating optional task handling
    let fork_size = 10;
    let mut branches: Vec<Vec<WorkflowTask>> = Vec::new();
    let mut join_on: Vec<String> = Vec::new();

    for i in 0..fork_size {
        let task_ref = format!("http_{}", i);

        // HTTP task marked as optional (won't fail the workflow on error)
        let http_task = WorkflowTask::http(
            &task_ref,
            "https://orkes-api-tester.orkesconductor.com/api2",
        )
        .optional();

        branches.push(vec![http_task]);
        join_on.push(task_ref);
    }

    // Create fork task
    let fork = WorkflowTask::fork("fork", branches);

    // Join script that checks if tasks completed (with or without errors)
    // This allows the join to complete even if some optional tasks failed
    let join_script = r#"
    (function(){
      let results = {};
      let pendingJoinsFound = false;
      if($.joinOn){
        $.joinOn.forEach((element)=>{
          if($[element] && $[element].status !== 'COMPLETED' && $[element] && $[element].status !== 'COMPLETED_WITH_ERRORS'){
            results[element] = $[element].status;
            pendingJoinsFound = true;
          }
        });
        if(pendingJoinsFound){
          return {
            "status":"IN_PROGRESS",
            "reasonForIncompletion":"Pending",
            "outputData":{
              "scriptResults": results
            }
          };
        }
        // To complete the Join - return true OR an object with status = 'COMPLETED' like above.
        return true;
      }
    })();
    "#;

    // Create join task with custom script
    let join = WorkflowTask::join_with_script("join", join_on.clone(), join_script);

    // Build workflow
    let workflow_def = WorkflowDef::new("fork_join_with_script_example")
        .with_description("Fork/Join with custom script for optional task handling")
        .with_version(1)
        .with_task(fork)
        .with_task(join)
        .with_output_param("results", "${join.output}");

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);
    println!(
        "\nThis workflow forks into {} parallel HTTP tasks.",
        fork_size
    );
    println!("The tasks are marked as optional and call an endpoint that returns 404.");
    println!("The custom join script allows completion even with failed optional tasks.\n");

    // Start workflow
    let request = StartWorkflowRequest::new(&workflow_def.name).with_version(1);

    let workflow_id = workflow_client.start_workflow(&request).await?;
    println!("Workflow started: {}", workflow_id);
    println!("Monitor at: {}/execution/{}", config.ui_host, workflow_id);

    // Wait a bit for execution
    println!("\nWaiting for workflow to complete...");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Check status
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    println!("\nWorkflow status: {:?}", workflow.status);

    // Count task statuses
    let mut completed = 0;
    let mut failed = 0;
    for task in &workflow.tasks {
        if task.reference_task_name.starts_with("http_") {
            match task.status {
                conductor::TaskStatus::Completed => {
                    completed += 1;
                }
                conductor::TaskStatus::Failed | conductor::TaskStatus::FailedWithTerminalError => {
                    failed += 1;
                }
                _ => {}
            }
        }
    }

    println!("\nTask summary:");
    println!("  - Completed: {}", completed);
    println!("  - Failed: {}", failed);

    if workflow.is_terminal() {
        println!("\nWorkflow completed despite optional task failures!");
    }

    Ok(())
}
