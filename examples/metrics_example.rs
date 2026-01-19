//! Metrics Example
//!
//! Demonstrates Prometheus metrics collection and HTTP endpoint.
//!
//! What it shows:
//! - Enabling Prometheus metrics collection
//! - Configuring the metrics HTTP server
//! - Available metrics (poll, execution, errors, etc.)
//! - Custom namespace and labels
//!
//! Run with: cargo run --example metrics_example
//!
//! Then visit: http://localhost:9090/metrics
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    metrics::MetricsSettings,
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

    // Load configuration
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config.clone())?;

    // Register workflow
    register_workflow(&client).await?;

    // Create task handler
    let mut handler = TaskHandler::new(config.clone())?;

    // ==============================
    // Configure Metrics
    // ==============================
    let metrics_settings = MetricsSettings::new()
        .with_http_port(9090) // Serve metrics on port 9090
        .with_metrics_path("/metrics") // Path for metrics endpoint
        .with_namespace("conductor"); // Prefix for all metrics

    handler.enable_metrics(metrics_settings);
    info!("Metrics enabled on http://localhost:9090/metrics");

    // ==============================
    // Create Workers
    // ==============================

    // Worker that processes quickly
    let quick_worker = FnWorker::new("quick_task", |_task: Task| async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(WorkerOutput::completed_with_result("quick done"))
    })
    .with_thread_count(10);

    // Worker with variable processing time
    let variable_worker = FnWorker::new("variable_task", |task: Task| async move {
        let delay_ms: u64 = task.get_input("delay_ms").unwrap_or(100);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        Ok(WorkerOutput::completed_with_result(format!(
            "completed in {}ms",
            delay_ms
        )))
    })
    .with_thread_count(5);

    // Worker that sometimes fails (for error metrics)
    let flaky_worker = FnWorker::new("flaky_task", |task: Task| async move {
        let fail_rate: f64 = task.get_input("fail_rate").unwrap_or(0.0);

        if rand_fail(fail_rate) {
            Ok(WorkerOutput::failed("Random failure for metrics testing"))
        } else {
            Ok(WorkerOutput::completed_with_result("success"))
        }
    })
    .with_thread_count(5);

    // Worker with large output (for size metrics)
    let large_output_worker = FnWorker::new("large_output_task", |task: Task| async move {
        let size: usize = task.get_input("output_size").unwrap_or(100);
        let data = "x".repeat(size);
        Ok(WorkerOutput::completed_with_result(data))
    })
    .with_thread_count(3);

    // Add workers
    handler.add_worker(quick_worker);
    handler.add_worker(variable_worker);
    handler.add_worker(flaky_worker);
    handler.add_worker(large_output_worker);

    // Start the handler
    info!("Starting task handler with metrics...");
    handler.start().await?;

    println!("\n{}", "=".repeat(70));
    println!("Prometheus Metrics Example");
    println!("{}", "=".repeat(70));
    println!("\nMetrics endpoint: http://localhost:9090/metrics");
    println!("Health endpoint:  http://localhost:9090/health");
    println!("\nAvailable Metrics:");
    println!("  Counter metrics:");
    println!("    - conductor_task_poll_total{{task_type}}");
    println!("    - conductor_task_poll_error_total{{task_type, error_type}}");
    println!("    - conductor_task_execute_error_total{{task_type, error_type}}");
    println!("    - conductor_task_update_error_total{{task_type}}");
    println!("    - conductor_task_paused_total{{task_type}}");
    println!("\n  Histogram metrics:");
    println!("    - conductor_task_poll_time_seconds{{task_type, status}}");
    println!("    - conductor_task_execute_time_seconds{{task_type, status}}");
    println!("\n  Gauge metrics:");
    println!("    - conductor_task_result_size_bytes{{task_type}}");
    println!("    - conductor_active_workers{{task_type}}");
    println!("\nWorkers:");
    println!("  - quick_task: Fast execution (~50ms)");
    println!("  - variable_task: Variable execution time");
    println!("  - flaky_task: Sometimes fails (for error metrics)");
    println!("  - large_output_task: Large output (for size metrics)");
    println!("{}", "=".repeat(70));

    // Start some workflows to generate metrics
    let workflow_client = client.workflow_client();

    info!("\nStarting workflows to generate metrics...");

    // Quick tasks
    for i in 0..5 {
        let request = StartWorkflowRequest::new("metrics_demo")
            .with_version(1)
            .with_input_value("task_type", "quick")
            .with_correlation_id(format!("quick_{}", i));
        let _ = workflow_client.start_workflow(&request).await;
    }

    // Variable time tasks
    for delay in [100, 200, 300, 500, 1000] {
        let request = StartWorkflowRequest::new("metrics_demo")
            .with_version(1)
            .with_input_value("task_type", "variable")
            .with_input_value("delay_ms", delay);
        let _ = workflow_client.start_workflow(&request).await;
    }

    // Flaky tasks (some will fail)
    for i in 0..10 {
        let request = StartWorkflowRequest::new("metrics_demo")
            .with_version(1)
            .with_input_value("task_type", "flaky")
            .with_input_value("fail_rate", 0.3) // 30% failure rate
            .with_correlation_id(format!("flaky_{}", i));
        let _ = workflow_client.start_workflow(&request).await;
    }

    // Large output tasks
    for size in [100, 1000, 5000, 10000] {
        let request = StartWorkflowRequest::new("metrics_demo")
            .with_version(1)
            .with_input_value("task_type", "large")
            .with_input_value("output_size", size);
        let _ = workflow_client.start_workflow(&request).await;
    }

    info!("Workflows started! Check http://localhost:9090/metrics for data.");

    // Wait a bit for tasks to process
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Print metrics programmatically
    if let Some(collector) = handler.metrics_collector() {
        info!("\nGathered metrics:");
        let metrics = collector.gather();
        // Print first 50 lines
        for line in metrics.lines().take(50) {
            if !line.starts_with('#') && !line.is_empty() {
                println!("  {}", line);
            }
        }
        println!("  ... (see http://localhost:9090/metrics for full output)");
    }

    // Keep running
    info!("\nMetrics server running. Visit http://localhost:9090/metrics");
    info!("Press Ctrl+C to stop...");
    tokio::signal::ctrl_c().await.ok();

    handler.stop().await?;
    info!("Done!");

    Ok(())
}

/// Simple random failure helper
fn rand_fail(rate: f64) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos as f64 / u32::MAX as f64) < rate
}

async fn register_workflow(client: &ConductorClient) -> Result<()> {
    let metadata = client.metadata_client();

    let workflow = WorkflowDef::new("metrics_demo")
        .with_description("Generates metrics for demonstration")
        .with_version(1)
        .with_task(
            WorkflowTask::switch("select_task", "$.task_type")
                .with_input_param("task_type", "${workflow.input.task_type}")
                .with_switch_case(
                    "quick",
                    vec![WorkflowTask::simple("quick_task", "quick_ref")],
                )
                .with_switch_case(
                    "variable",
                    vec![WorkflowTask::simple("variable_task", "variable_ref")
                        .with_input_param("delay_ms", "${workflow.input.delay_ms}")],
                )
                .with_switch_case(
                    "flaky",
                    vec![WorkflowTask::simple("flaky_task", "flaky_ref")
                        .with_input_param("fail_rate", "${workflow.input.fail_rate}")],
                )
                .with_switch_case(
                    "large",
                    vec![WorkflowTask::simple("large_output_task", "large_ref")
                        .with_input_param("output_size", "${workflow.input.output_size}")],
                )
                .with_default_case(vec![WorkflowTask::simple("quick_task", "default_ref")]),
        )
        .with_output_param("result", "${select_task.output}");

    info!("Registering workflow: {}", workflow.name);
    metadata
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    Ok(())
}
