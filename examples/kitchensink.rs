//! Kitchen Sink Example
//!
//! Comprehensive example demonstrating all major workflow task types and patterns.
//!
//! What it demonstrates:
//! - HTTP Task: Make external API calls
//! - JavaScript (Inline) Task: Execute inline JavaScript code
//! - JSON JQ Task: Transform JSON using JQ queries
//! - Switch Task: Conditional branching based on values
//! - Wait Task: Pause workflow execution
//! - Set Variable Task: Store values in workflow variables
//! - Terminate Task: End workflow with specific status
//! - Fork/Join Task: Parallel execution
//! - Custom Worker Task: Execute Rust business logic
//!
//! Run with: cargo run --example kitchensink
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{StartWorkflowRequest, Task, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
use std::collections::HashMap;
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

    // Load configuration from environment
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config.clone())?;

    // Register the workflow definition
    register_workflow(&client).await?;

    // Create workers
    let mut handler = TaskHandler::new(config.clone())?;

    // Route worker - handles routing based on country
    let route_worker = FnWorker::new("route", |task: Task| async move {
        let country = task
            .get_input_string("country")
            .unwrap_or_else(|| "Unknown".to_string());

        info!("Routing packages to: {}", country);

        let mut output = HashMap::new();
        output.insert(
            "result".to_string(),
            serde_json::json!(format!("Routing packages to {}", country)),
        );
        output.insert("country".to_string(), serde_json::json!(country));

        Ok(WorkerOutput::Completed(output))
    });

    // Greet worker - simple greeting
    let greet_worker = FnWorker::new("greet", |task: Task| async move {
        let name = task
            .get_input_string("name")
            .unwrap_or_else(|| "World".to_string());

        info!("Greeting: {}", name);

        let mut output = HashMap::new();
        output.insert(
            "greeting".to_string(),
            serde_json::json!(format!("Hello, {}!", name)),
        );

        Ok(WorkerOutput::Completed(output))
    });

    handler.add_worker(route_worker);
    handler.add_worker(greet_worker);
    handler.start().await?;

    // Execute the workflow
    info!("Starting kitchensink workflow...");
    let workflow_client = client.workflow_client();

    let request = StartWorkflowRequest::new("kitchensink")
        .with_version(1)
        .with_input_value("name", "Orkes")
        .with_input_value("country", "US");

    // Execute synchronously with a wait time
    let workflow = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await?;

    info!("Workflow ID: {}", workflow.workflow_id);
    info!("Workflow status: {:?}", workflow.status);

    if let Some(output) = workflow.output.get("greetings") {
        info!("Workflow output (greetings): {}", output);
    }
    if let Some(output) = workflow.output.get("routing_result") {
        info!("Workflow output (routing): {}", output);
    }

    info!(
        "View execution at: {}",
        config.execution_url(&workflow.workflow_id)
    );

    // Stop the handler
    handler.stop().await?;

    info!("Kitchen sink example completed!");
    Ok(())
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    // JavaScript task - creates a greeting message
    let js_script = r#"
        (function greetings() {
            return {
                "text": "hello " + $.name,
                "url": "https://orkes-api-tester.orkesconductor.com/api"
            };
        })();
    "#;

    let js_task = WorkflowTask::inline("hello_script", js_script)
        .with_input_param("name", "${workflow.input.name}");

    // HTTP task - calls an external API (used in fork branch instead)
    // Note: We use http_branch in the fork for parallel execution

    // Wait task - pauses for 2 seconds (used in fork branch instead)
    // Note: We use wait_branch in the fork for parallel execution

    // JSON JQ Transform task - merges two arrays
    let jq_script = "{ key3: (.key1.value1 + .key2.value2) }";
    let jq_task = WorkflowTask::json_jq_transform("jq_process", jq_script)
        .with_input_param("key1", serde_json::json!({"value1": ["a", "b"]}))
        .with_input_param("key2", serde_json::json!({"value2": ["c", "d"]}));

    // Set Variable task - stores workflow variables
    let set_var_task = WorkflowTask::set_variable("set_wf_var")
        .with_input_param("var1", "value1")
        .with_input_param("var2", 42)
        .with_input_param("var3", serde_json::json!(["a", "b", "c"]));

    // Simple greet task
    let greet_task = WorkflowTask::simple("greet", "greet_ref")
        .with_input_param("name", "${workflow.input.name}");

    // Switch task - routes based on country
    let us_route = WorkflowTask::simple("route", "us_routing")
        .with_input_param("country", "${workflow.input.country}");

    let ca_route = WorkflowTask::simple("route", "ca_routing")
        .with_input_param("country", "${workflow.input.country}");

    // Terminate task for unsupported countries
    let terminate_task = WorkflowTask::terminate("bad_country", "FAILED")
        .with_termination_reason("Unsupported country");

    let switch_task = WorkflowTask::switch(
        "decide_route",
        "$.country == 'US' ? 'US' : ($.country == 'CA' ? 'CA' : 'default')",
    )
    .with_input_param("country", "${workflow.input.country}")
    .with_switch_case("US", vec![us_route])
    .with_switch_case("CA", vec![ca_route])
    .with_default_case(vec![terminate_task]);

    // Fork task for parallel execution (http + wait in parallel)
    let http_branch = WorkflowTask::http(
        "parallel_http",
        "https://orkes-api-tester.orkesconductor.com/api",
    );
    let wait_branch = WorkflowTask::wait_duration("parallel_wait", "1 seconds");

    let fork_task =
        WorkflowTask::fork("parallel_tasks", vec![vec![http_branch], vec![wait_branch]]);

    let join_task = WorkflowTask::join(
        "join_parallel",
        vec!["parallel_http".to_string(), "parallel_wait".to_string()],
    );

    // Build the workflow
    let workflow = WorkflowDef::new("kitchensink")
        .with_description("Kitchen sink example demonstrating all task types")
        .with_version(1)
        .with_task(greet_task)
        .with_task(js_task)
        .with_task(fork_task)
        .with_task(join_task)
        .with_task(jq_task)
        .with_task(set_var_task)
        .with_task(switch_task)
        .with_output_param("greetings", "${greet_ref.output.greeting}")
        .with_output_param("routing_result", "${decide_route.output.result}");

    // Register the workflow
    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
