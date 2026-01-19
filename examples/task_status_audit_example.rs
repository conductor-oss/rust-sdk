//! Task Status Change Audit Example
//!
//! This example demonstrates how to use state change events to trigger
//! audit logging when task statuses change. This is useful for monitoring,
//! compliance, and debugging workflows.
//! It mirrors the Python SDK's orkes/task_status_change_audit.py example.
//!
//! Prerequisites:
//! - Conductor server running
//! - CONDUCTOR_SERVER_URL environment variable set

use conductor::worker::{FnWorker, WorkerOutput};
use conductor::{
    Configuration, EmbeddedTaskDef, OrkesClients, StartWorkflowRequest, StateChangeConfig,
    StateChangeEvent, StateChangeEventType, TaskDef, TaskHandler, WorkflowDef, WorkflowTask,
};

/// Audit log worker - receives notifications on task state changes
async fn audit_log(task: conductor::Task) -> conductor::Result<WorkerOutput> {
    let workflow_input = task
        .get_input::<serde_json::Value>("workflow_input")
        .unwrap_or(serde_json::Value::Null);
    let status = task.get_input_string("status").unwrap_or_default();
    let name = task.get_input_string("name").unwrap_or_default();

    println!(
        "AUDIT: Task '{}' is in '{}' status. Workflow input: {:?}",
        name, status, workflow_input
    );

    Ok(WorkerOutput::complete())
}

/// Simple task 1 - completes successfully
async fn simple_task_1(_task: conductor::Task) -> conductor::Result<WorkerOutput> {
    println!("Executing simple_task_1 - will complete successfully");
    Ok(WorkerOutput::completed_with_result(serde_json::json!("OK")))
}

/// Simple task 2 - fails with terminal error
async fn simple_task_2(_task: conductor::Task) -> conductor::Result<WorkerOutput> {
    println!("Executing simple_task_2 - will fail with terminal error");

    Ok(WorkerOutput::failed("Intentional failure for demo"))
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
    handler.add_worker(FnWorker::new("audit_log", audit_log));
    handler.add_worker(FnWorker::new("simple_task_1", simple_task_1));
    handler.add_worker(FnWorker::new("simple_task_2", simple_task_2));
    handler.start().await?;

    // Register task definitions
    let audit_task_def = TaskDef::new("audit_log");
    let task1_def = TaskDef::new("simple_task_1");
    let task2_def =
        TaskDef::new("simple_task_2").with_retry(0, conductor::models::RetryLogic::Fixed, 0); // No retries so it fails immediately

    metadata_client.register_task_def(&audit_task_def).await?;
    metadata_client.register_task_def(&task1_def).await?;
    metadata_client.register_task_def(&task2_def).await?;

    // Build workflow with state change events

    // Task 1: Trigger audit on start
    let task1 = WorkflowTask::simple("simple_task_1", "simple_task_1_ref").with_state_change(
        StateChangeConfig::new()
            .on_event(StateChangeEventType::OnStart)
            .with_event(
                StateChangeEvent::new("audit_log")
                    .with_payload("workflow_input", serde_json::json!("${workflow.input}"))
                    .with_payload("status", serde_json::json!("${simple_task_1_ref.status}"))
                    .with_payload("name", serde_json::json!("simple_task_1_ref")),
            ),
    );

    // Task 2: Trigger audit on scheduled, start, and failed
    let task2 = WorkflowTask::simple("simple_task_2", "simple_task_2_ref")
        .with_task_definition(EmbeddedTaskDef::new("simple_task_2").with_retry_count(0))
        .with_state_change(
            StateChangeConfig::new()
                .on_event(StateChangeEventType::OnScheduled)
                .on_event(StateChangeEventType::OnStart)
                .on_event(StateChangeEventType::OnFailed)
                .with_event(
                    StateChangeEvent::new("audit_log")
                        .with_payload("workflow_input", serde_json::json!("${workflow.input}"))
                        .with_payload("status", serde_json::json!("${simple_task_2_ref.status}"))
                        .with_payload("name", serde_json::json!("simple_task_2_ref")),
                ),
        );

    let workflow_def = WorkflowDef::new("test_audit_logs")
        .with_description("Demonstrates task state change audit events")
        .with_version(1)
        .with_task(task1)
        .with_task(task2);

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);
    println!("\nThis workflow demonstrates state change events:");
    println!("  - simple_task_1: Triggers audit_log on START");
    println!("  - simple_task_2: Triggers audit_log on SCHEDULED, START, and FAILED");
    println!("\nWatch the console for AUDIT messages as tasks change state.\n");

    // Start workflow
    let request = StartWorkflowRequest::new(&workflow_def.name)
        .with_version(1)
        .with_input_value("a", "aa")
        .with_input_value("b", "bb")
        .with_input_value("c", 42);

    let workflow_id = workflow_client.start_workflow(&request).await?;
    println!("Workflow started: {}", workflow_id);

    // Wait for workflow and audit tasks to complete
    println!("\nWaiting for workflow execution...\n");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Check final status
    let workflow = workflow_client.get_workflow(&workflow_id, true).await?;
    println!("\nFinal workflow status: {:?}", workflow.status);

    println!("\nTask execution summary:");
    for task in &workflow.tasks {
        println!(
            "  - {} ({:?}): {:?}",
            task.reference_task_name, task.task_type, task.status
        );
    }

    // Cleanup
    handler.stop().await?;

    println!("\nExample complete. The audit_log tasks were triggered automatically");
    println!("based on the state change configuration.");

    Ok(())
}
