// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::{ConductorClient, TestWorkflowRequest},
    configuration::Configuration,
    error::Result,
    models::{WorkflowDef, WorkflowTask},
};
use std::collections::HashMap;
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
    let client = ConductorClient::new(config.clone())?;
    let workflow_client = client.workflow_client();
    let metadata = client.metadata_client();

    // ==============================
    // Register a Test Workflow
    // ==============================
    info!("\n=== Registering Test Workflow ===");

    let workflow = WorkflowDef::new("test_workflow_example")
        .with_description("Workflow for testing functionality")
        .with_version(1)
        // Task 1: Process order
        .with_task(
            WorkflowTask::simple("process_order", "process_order_ref")
                .with_input_param("order_id", "${workflow.input.order_id}")
                .with_input_param("amount", "${workflow.input.amount}"),
        )
        // Task 2: Validate payment (depends on process_order)
        .with_task(
            WorkflowTask::simple("validate_payment", "validate_payment_ref")
                .with_input_param("order_id", "${process_order_ref.output.order_id}")
                .with_input_param("validated_amount", "${process_order_ref.output.total}"),
        )
        // Task 3: Send notification
        .with_task(
            WorkflowTask::simple("send_notification", "send_notification_ref")
                .with_input_param("order_id", "${process_order_ref.output.order_id}")
                .with_input_param("status", "${validate_payment_ref.output.status}"),
        )
        // Output
        .with_output_param("order_result", "${process_order_ref.output}")
        .with_output_param("payment_result", "${validate_payment_ref.output}")
        .with_output_param("notification_result", "${send_notification_ref.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    // ==============================
    // Test 1: Basic Workflow Test
    // ==============================
    info!("\n=== Test 1: Basic Workflow Test ===");

    // Create mock outputs for each task
    let mut process_order_output = HashMap::new();
    process_order_output.insert("order_id".to_string(), serde_json::json!("ORD-123"));
    process_order_output.insert("total".to_string(), serde_json::json!(99.99));
    process_order_output.insert("items".to_string(), serde_json::json!(["item1", "item2"]));

    let mut validate_payment_output = HashMap::new();
    validate_payment_output.insert("status".to_string(), serde_json::json!("SUCCESS"));
    validate_payment_output.insert("transaction_id".to_string(), serde_json::json!("TXN-456"));

    let mut send_notification_output = HashMap::new();
    send_notification_output.insert("sent".to_string(), serde_json::json!(true));
    send_notification_output.insert("channel".to_string(), serde_json::json!("email"));

    // Create workflow input
    let mut workflow_input = HashMap::new();
    workflow_input.insert("order_id".to_string(), serde_json::json!("ORD-123"));
    workflow_input.insert("amount".to_string(), serde_json::json!(99.99));

    // Build test request
    let test_request = TestWorkflowRequest::new("test_workflow_example")
        .with_version(1)
        .with_input(workflow_input)
        .with_mock_output("process_order_ref", process_order_output)
        .with_mock_output("validate_payment_ref", validate_payment_output)
        .with_mock_output("send_notification_ref", send_notification_output);

    info!("Running workflow test...");
    let result = workflow_client.test_workflow(&test_request).await?;

    info!("Test result status: {:?}", result.status);
    info!("Test workflow ID: {}", result.workflow_id);

    if result.is_successful() {
        info!("Workflow test PASSED!");
        info!("Output:");
        for (key, value) in &result.output {
            info!("  {}: {}", key, value);
        }
    } else {
        info!("Workflow test FAILED!");
        if let Some(reason) = &result.reason_for_incompletion {
            info!("Reason: {}", reason);
        }
    }

    // ==============================
    // Test 2: Test with Failure Scenario
    // ==============================
    info!("\n=== Test 2: Payment Failure Scenario ===");

    // Mock payment validation failure
    let mut failed_payment_output = HashMap::new();
    failed_payment_output.insert("status".to_string(), serde_json::json!("FAILED"));
    failed_payment_output.insert("error".to_string(), serde_json::json!("Insufficient funds"));

    let test_request_failure = TestWorkflowRequest::new("test_workflow_example")
        .with_version(1)
        .with_input({
            let mut input = HashMap::new();
            input.insert("order_id".to_string(), serde_json::json!("ORD-789"));
            input.insert("amount".to_string(), serde_json::json!(999999.99));
            input
        })
        .with_mock_output("process_order_ref", {
            let mut output = HashMap::new();
            output.insert("order_id".to_string(), serde_json::json!("ORD-789"));
            output.insert("total".to_string(), serde_json::json!(999999.99));
            output
        })
        .with_mock_output("validate_payment_ref", failed_payment_output)
        .with_mock_output("send_notification_ref", {
            let mut output = HashMap::new();
            output.insert("sent".to_string(), serde_json::json!(true));
            output.insert(
                "message".to_string(),
                serde_json::json!("Payment failed notification"),
            );
            output
        });

    info!("Running failure scenario test...");
    let failure_result = workflow_client.test_workflow(&test_request_failure).await?;

    info!("Failure test status: {:?}", failure_result.status);
    if let Some(payment_result) = failure_result.output.get("payment_result") {
        info!("Payment result: {}", payment_result);
    }

    // ==============================
    // Test 3: Test with Inline Workflow Definition
    // ==============================
    info!("\n=== Test 3: Test with Inline Workflow ===");

    // Create a workflow inline (not registered)
    let inline_workflow = WorkflowDef::new("inline_test_workflow")
        .with_description("Inline workflow for testing")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("calculate", "calc_ref")
                .with_input_param("a", "${workflow.input.a}")
                .with_input_param("b", "${workflow.input.b}"),
        )
        .with_output_param("result", "${calc_ref.output.sum}");

    let inline_test = TestWorkflowRequest::new("inline_test_workflow")
        .with_version(1)
        .with_workflow_def(inline_workflow)
        .with_input({
            let mut input = HashMap::new();
            input.insert("a".to_string(), serde_json::json!(10));
            input.insert("b".to_string(), serde_json::json!(20));
            input
        })
        .with_mock_output("calc_ref", {
            let mut output = HashMap::new();
            output.insert("sum".to_string(), serde_json::json!(30));
            output
        });

    info!("Running inline workflow test...");
    let inline_result = workflow_client.test_workflow(&inline_test).await?;

    info!("Inline test status: {:?}", inline_result.status);
    if let Some(result) = inline_result.output.get("result") {
        info!("Calculation result: {}", result);
    }

    // ==============================
    // Cleanup
    // ==============================
    info!("\n=== Cleanup ===");

    metadata
        .delete_workflow_def("test_workflow_example", 1)
        .await?;
    info!("Deleted test workflow");

    info!("\nWorkflow testing example completed!");
    Ok(())
}
