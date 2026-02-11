//! Workflow Status Listener Example
//!
//! Demonstrates enabling external status listeners for workflow state changes.
//!
//! ## What It Does
//! - Creates a workflow with HTTP task
//! - Enables a Kafka/SQS status listener
//! - Registers the workflow with listener configuration
//! - Status changes will be published to the specified sink
//!
//! ## Use Cases
//! - Real-time workflow monitoring via message queues
//! - Integrating workflows with external systems (Kafka, SQS, etc.)
//! - Building event-driven architectures
//! - Audit logging and compliance tracking
//! - Custom notifications on workflow state changes
//! - Analytics and metrics collection
//!
//! ## Status Events Published
//! - Workflow started
//! - Workflow completed
//! - Workflow failed
//! - Workflow paused
//! - Workflow resumed
//! - Workflow terminated
//! - Task status changes
//!
//! ## Sink Formats
//! - Kafka: `kafka:<topic_name>`
//! - SQS: `sqs:<queue_url>`
//! - NATS: `nats:<subject>`
//! - AMQP: `amqp_exchange:<exchange_name>`
//!
//! ## Environment Variables
//! ```bash
//! export CONDUCTOR_SERVER_URL="https://your-conductor-server/api"
//! export CONDUCTOR_AUTH_KEY="your-key"
//! export CONDUCTOR_AUTH_SECRET="your-secret"
//! ```
//!
//! ## Run
//! ```bash
//! cargo run --example workflow_status_listener
//! ```

use conductor_rust::{ConductorClient, Configuration, WorkflowDef, WorkflowTask};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Workflow Status Listener Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config).await?;

    // Create a simple workflow with HTTP task
    let workflow_name = "rust_workflow_status_listener_demo";

    // Build the workflow definition
    let workflow = WorkflowDef::new(workflow_name)
        .with_description("Demo workflow with status listener enabled")
        .with_version(1)
        // Add a simple HTTP task
        .with_task(
            WorkflowTask::http("http_ref", "https://orkes-api-tester.orkesconductor.com/api")
                .with_input_param("method", "GET")
                .with_description("Simple HTTP call to demonstrate status events"),
        )
        // Enable workflow status listener - events will be published to Kafka topic
        .with_status_listener_enabled(true);

    // Note: The status listener sink is configured at the workflow execution level
    // or via workflow input parameters in production scenarios

    println!("Workflow Configuration:");
    println!("  Name: {}", workflow.name);
    println!("  Description: {:?}", workflow.description);
    println!(
        "  Status Listener Enabled: {}",
        workflow.workflow_status_listener_enabled
    );
    println!();

    // Register the workflow
    println!("Registering workflow with status listener...");
    let metadata_client = client.metadata_client();
    metadata_client.register_workflow_def(&workflow, true).await?;
    println!("  Workflow registered: {}", workflow_name);

    // Display status listener configuration
    println!("\n{}", "=".repeat(80));
    println!("STATUS LISTENER CONFIGURATION");
    println!("{}", "=".repeat(80));
    println!();
    println!("When this workflow executes, status events will be published to");
    println!("the configured sink. Common sink formats:");
    println!();
    println!("  Kafka:    kafka:workflow-events-topic");
    println!("  SQS:      sqs:https://sqs.us-east-1.amazonaws.com/123456/queue");
    println!("  NATS:     nats:workflow.events");
    println!("  AMQP:     amqp_exchange:workflow-events");
    println!();

    // Display status events that will be published
    println!("Events Published:");
    println!("  - onStart: When workflow execution begins");
    println!("  - onComplete: When workflow completes successfully");
    println!("  - onFail: When workflow fails");
    println!("  - onPause: When workflow is paused");
    println!("  - onResume: When workflow is resumed");
    println!("  - onTerminate: When workflow is terminated");
    println!();

    // Show example event payload
    println!("Example Event Payload:");
    println!(
        r#"  {{
    "workflowId": "abc123-def456",
    "workflowName": "{}",
    "status": "COMPLETED",
    "startTime": 1703444400000,
    "endTime": 1703444401000,
    "input": {{}},
    "output": {{}},
    "reason": null
  }}"#,
        workflow_name
    );
    println!();

    // Demonstrate task-level status events
    println!("{}", "=".repeat(80));
    println!("TASK-LEVEL STATUS EVENTS");
    println!("{}", "=".repeat(80));
    println!();
    println!("You can also configure task-level status events using onStateChange.");
    println!("This allows fine-grained control over which task events trigger notifications.");
    println!();

    // Create a workflow with task-level state change configuration
    let audit_workflow = WorkflowDef::new("rust_task_status_audit_demo")
        .with_description("Demo workflow with task-level status audit")
        .with_version(1)
        .with_task(
            WorkflowTask::http("http_task_ref", "https://orkes-api-tester.orkesconductor.com/api")
                .with_description("HTTP task with audit events")
                // Configure state change events for this task
                .with_state_change(
                    conductor_rust::StateChangeConfig::new()
                        .on_event(conductor_rust::StateChangeEventType::OnScheduled)
                        .on_event(conductor_rust::StateChangeEventType::OnStart)
                        .on_event(conductor_rust::StateChangeEventType::OnCompleted)
                        .on_event(conductor_rust::StateChangeEventType::OnFailed)
                        .with_event(
                            conductor_rust::StateChangeEvent::new("kafka:task-audit-events")
                                .with_payload(
                                    "taskRef",
                                    serde_json::json!("${task.referenceTaskName}"),
                                )
                                .with_payload("status", serde_json::json!("${task.status}"))
                                .with_payload("workflowId", serde_json::json!("${workflow.workflowId}")),
                        ),
                ),
        )
        .with_status_listener_enabled(true);

    println!("Task State Change Configuration:");
    println!("  Events: onScheduled, onStart, onCompleted, onFailed");
    println!("  Sink: kafka:task-audit-events");
    println!("  Payload: taskRef, status, workflowId");
    println!();

    // Register the audit workflow
    metadata_client.register_workflow_def(&audit_workflow, true).await?;
    println!("  Audit workflow registered: rust_task_status_audit_demo");

    // Clean up
    println!("\n{}", "=".repeat(80));
    println!("CLEANUP");
    println!("{}", "=".repeat(80));
    println!();
    println!("Cleaning up created resources...");

    match metadata_client
        .unregister_workflow_def(workflow_name, 1)
        .await
    {
        Ok(_) => println!("  Deleted: {}", workflow_name),
        Err(e) => println!("  Could not delete {}: {}", workflow_name, e),
    }

    match metadata_client
        .unregister_workflow_def("rust_task_status_audit_demo", 1)
        .await
    {
        Ok(_) => println!("  Deleted: rust_task_status_audit_demo"),
        Err(e) => println!(
            "  Could not delete rust_task_status_audit_demo: {}",
            e
        ),
    }

    println!("\n  Status listener example completed!");
    println!();
    println!("Next Steps:");
    println!("  1. Configure your message queue (Kafka, SQS, etc.)");
    println!("  2. Set up the event sink in Conductor");
    println!("  3. Create consumers to process workflow events");
    println!("  4. Build dashboards, alerts, or audit systems");

    Ok(())
}

// Extension trait to add status listener support to WorkflowDef
trait WorkflowDefExt {
    fn with_status_listener_enabled(self, enabled: bool) -> Self;
}

impl WorkflowDefExt for WorkflowDef {
    fn with_status_listener_enabled(mut self, enabled: bool) -> Self {
        self.workflow_status_listener_enabled = enabled;
        self
    }
}
