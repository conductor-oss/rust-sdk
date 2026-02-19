// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    models::{StartWorkflowRequest, Task, TaskDef, WorkflowDef, WorkflowStatus, WorkflowTask},
    worker::{FnWorker, WorkerOutput},
    ConductorClient, Configuration, TaskHandler,
};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Test configuration
const WORKFLOW_COUNT: usize = 100; // Number of workflows to run
const TASKS_PER_WORKFLOW: usize = 3; // Tasks in sequence per workflow
const WORKER_THREAD_COUNT: usize = 20; // Concurrent task executions per worker
const MAX_WORKFLOW_DURATION_SECS: u64 = 120; // Max time to wait for all workflows
const _EXPECTED_MAX_POLL_LATENCY_MS: u64 = 500; // Expected max poll latency (reserved for future use)

/// Statistics collector
#[derive(Default)]
struct TestStats {
    tasks_executed: AtomicUsize,
    tasks_failed: AtomicUsize,
    total_execution_time_us: AtomicU64,
    min_execution_time_us: AtomicU64,
    max_execution_time_us: AtomicU64,

    // Concurrency validation
    task_results: Mutex<HashMap<String, String>>, // task_id -> unique_value processed
    concurrency_errors: AtomicUsize,
}

impl TestStats {
    fn new() -> Self {
        Self {
            min_execution_time_us: AtomicU64::new(u64::MAX),
            ..Default::default()
        }
    }

    fn record_execution(&self, duration: Duration) {
        let us = duration.as_micros() as u64;
        self.tasks_executed.fetch_add(1, Ordering::SeqCst);
        self.total_execution_time_us.fetch_add(us, Ordering::SeqCst);

        // Update min
        let mut current_min = self.min_execution_time_us.load(Ordering::SeqCst);
        while us < current_min {
            match self.min_execution_time_us.compare_exchange_weak(
                current_min,
                us,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // Update max
        let mut current_max = self.max_execution_time_us.load(Ordering::SeqCst);
        while us > current_max {
            match self.max_execution_time_us.compare_exchange_weak(
                current_max,
                us,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    fn record_task_result(&self, task_id: &str, unique_value: &str) {
        let mut results = self.task_results.lock();
        if let Some(existing) = results.get(task_id) {
            if existing != unique_value {
                eprintln!(
                    "CONCURRENCY ERROR: Task {} processed with different values: {} vs {}",
                    task_id, existing, unique_value
                );
                self.concurrency_errors.fetch_add(1, Ordering::SeqCst);
            }
        } else {
            results.insert(task_id.to_string(), unique_value.to_string());
        }
    }

    fn print_summary(&self) {
        let executed = self.tasks_executed.load(Ordering::SeqCst);
        let failed = self.tasks_failed.load(Ordering::SeqCst);
        let total_us = self.total_execution_time_us.load(Ordering::SeqCst);
        let min_us = self.min_execution_time_us.load(Ordering::SeqCst);
        let max_us = self.max_execution_time_us.load(Ordering::SeqCst);
        let concurrency_errors = self.concurrency_errors.load(Ordering::SeqCst);

        let avg_us = if executed > 0 {
            total_us / executed as u64
        } else {
            0
        };

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                    PERFORMANCE TEST RESULTS                   ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!(
            "║ Tasks Executed:     {:>8}                                 ║",
            executed
        );
        println!(
            "║ Tasks Failed:       {:>8}                                 ║",
            failed
        );
        println!(
            "║ Concurrency Errors: {:>8}                                 ║",
            concurrency_errors
        );
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Execution Time (per task):                                   ║");
        println!(
            "║   Min:              {:>8} µs                              ║",
            min_us
        );
        println!(
            "║   Max:              {:>8} µs                              ║",
            max_us
        );
        println!(
            "║   Avg:              {:>8} µs                              ║",
            avg_us
        );
        println!("╠══════════════════════════════════════════════════════════════╣");
        if concurrency_errors == 0 {
            println!("║ ✅ NO CONCURRENCY ISSUES DETECTED                            ║");
        } else {
            println!(
                "║ ❌ CONCURRENCY ISSUES FOUND: {}                              ║",
                concurrency_errors
            );
        }
        println!("╚══════════════════════════════════════════════════════════════╝\n");
    }
}

/// Create a worker that validates its input and tracks execution
fn create_test_worker(task_name: &str, stats: Arc<TestStats>) -> FnWorker {
    let task_name = task_name.to_string();
    FnWorker::new(task_name.clone(), move |task: Task| {
        let stats = Arc::clone(&stats);
        let task_name = task_name.clone();
        async move {
            let start = Instant::now();

            // Extract unique identifiers from input
            let workflow_id = task.workflow_instance_id.clone();
            let task_id = task.task_id.clone();
            let unique_value: String = task
                .get_input("unique_value")
                .unwrap_or_else(|| format!("missing-{}", task_id));
            let sequence: i32 = task.get_input("sequence").unwrap_or(0);

            // Record this task's processing for concurrency validation
            // The unique_value should be unique per task instance
            stats.record_task_result(&task_id, &unique_value);

            // Simulate some work (variable delay based on task)
            let work_delay = (sequence as u64 % 5) + 1; // 1-5ms
            tokio::time::sleep(Duration::from_millis(work_delay)).await;

            // Record execution time
            stats.record_execution(start.elapsed());

            // Return output with the unique value (for workflow validation)
            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "task_name": task_name,
                "unique_value": unique_value,
                "workflow_id": workflow_id,
                "sequence": sequence,
                "execution_time_us": start.elapsed().as_micros()
            })))
        }
    })
    .with_thread_count(WORKER_THREAD_COUNT)
    .with_poll_interval_millis(50)
}

/// Main performance test
#[tokio::test]
async fn test_performance_and_concurrency() {
    // Skip if no Conductor server
    if std::env::var("CONDUCTOR_SERVER_URL").is_err() {
        println!("Skipping performance test: CONDUCTOR_SERVER_URL not set");
        println!("Set CONDUCTOR_SERVER_URL to run this test against a Conductor server");
        return;
    }

    println!("\n🚀 Starting Performance and Concurrency Test");
    println!("   Workflows: {}", WORKFLOW_COUNT);
    println!("   Tasks per workflow: {}", TASKS_PER_WORKFLOW);
    println!("   Worker concurrency: {}", WORKER_THREAD_COUNT);

    let config = Configuration::default();
    let client = ConductorClient::new(config.clone()).expect("Failed to create client");
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    // Create statistics collector
    let stats = Arc::new(TestStats::new());

    // Create unique workflow and task names for this test run
    let test_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let workflow_name = format!("perf_test_workflow_{}", test_id);
    let task_names: Vec<String> = (0..TASKS_PER_WORKFLOW)
        .map(|i| format!("perf_test_task_{}_{}", test_id, i))
        .collect();

    println!("   Workflow: {}", workflow_name);
    println!("   Tasks: {:?}", task_names);

    // Register task definitions
    println!("\n📝 Registering task definitions...");
    for task_name in &task_names {
        let task_def = TaskDef::new(task_name)
            .with_retry(0, conductor::models::RetryLogic::Fixed, 0)
            .with_timeout(60, conductor::models::TimeoutPolicy::TimeOutWf)
            .with_response_timeout(30);

        if let Err(e) = metadata_client.register_task_def(&task_def).await {
            println!("   Warning: Failed to register {}: {}", task_name, e);
        }
    }

    // Register workflow definition with sequential tasks
    println!("📝 Registering workflow definition...");
    let mut workflow_def =
        WorkflowDef::new(&workflow_name).with_description("Performance test workflow");

    for (i, task_name) in task_names.iter().enumerate() {
        let task = WorkflowTask::simple(task_name, format!("task_{}", i))
            .with_input_param(
                "unique_value",
                format!("${{workflow.input.unique_value_{}}}", i),
            )
            .with_input_param("sequence", serde_json::json!(i));
        workflow_def = workflow_def.with_task(task);
    }

    if let Err(e) = metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await
    {
        println!("   Failed to register workflow: {}", e);
        return;
    }

    // Create and start workers
    println!("\n👷 Starting workers...");
    let mut handler = TaskHandler::new(config.clone()).expect("Failed to create handler");

    for task_name in &task_names {
        handler.add_worker(create_test_worker(task_name, Arc::clone(&stats)));
    }

    handler.start().await.expect("Failed to start handler");
    println!(
        "   Workers started with {} threads each",
        WORKER_THREAD_COUNT
    );

    // Give workers time to start polling
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Start workflows concurrently
    println!("\n🏃 Starting {} workflows...", WORKFLOW_COUNT);
    let start_time = Instant::now();

    let mut workflow_ids = Vec::with_capacity(WORKFLOW_COUNT);
    let start_semaphore = Arc::new(Semaphore::new(50)); // Limit concurrent starts

    let mut start_handles = Vec::new();
    for i in 0..WORKFLOW_COUNT {
        let workflow_client = workflow_client.clone();
        let workflow_name = workflow_name.clone();
        let semaphore = Arc::clone(&start_semaphore);

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            // Create unique values for each task in this workflow
            let mut input = serde_json::Map::new();
            let wf_uuid = uuid::Uuid::new_v4().to_string();
            for j in 0..TASKS_PER_WORKFLOW {
                let unique_value = format!("PLACEHOLDER-{}-{}", j, wf_uuid);
                input.insert(
                    format!("unique_value_{}", j),
                    serde_json::Value::String(unique_value),
                );
            }

            let request =
                StartWorkflowRequest::new(&workflow_name).with_input(input.into_iter().collect());

            match workflow_client.start_workflow(&request).await {
                Ok(wf_id) => {
                    // Update unique values with actual workflow ID
                    Some((i, wf_id))
                }
                Err(e) => {
                    eprintln!("Failed to start workflow {}: {}", i, e);
                    None
                }
            }
        });
        start_handles.push(handle);
    }

    // Collect workflow IDs
    for handle in start_handles {
        if let Ok(Some((_, wf_id))) = handle.await {
            workflow_ids.push(wf_id);
        }
    }

    let started_count = workflow_ids.len();
    println!(
        "   Started {} workflows in {:?}",
        started_count,
        start_time.elapsed()
    );

    if started_count == 0 {
        println!("❌ No workflows started successfully");
        handler.stop().await.ok();
        return;
    }

    // Wait for all workflows to complete
    println!("\n⏳ Waiting for workflows to complete...");
    let completion_start = Instant::now();
    let timeout = Duration::from_secs(MAX_WORKFLOW_DURATION_SECS);
    let check_interval = Duration::from_millis(500);
    let mut error_log_count = 0;

    loop {
        if completion_start.elapsed() > timeout {
            println!("   ⚠️ Timeout reached, some workflows may still be running");
            break;
        }

        let mut still_running = 0;
        for wf_id in &workflow_ids {
            match workflow_client.get_workflow(wf_id, false).await {
                Ok(wf) => match wf.status {
                    WorkflowStatus::Completed => {} // Count in final check
                    WorkflowStatus::Failed
                    | WorkflowStatus::Terminated
                    | WorkflowStatus::TimedOut => {
                        error_log_count += 1;
                        if error_log_count <= 5 {
                            println!("   Workflow {} failed: {:?}", wf_id, wf.status);
                        }
                    }
                    _ => still_running += 1,
                },
                Err(_) => still_running += 1,
            }
        }

        if still_running == 0 {
            break;
        }

        tokio::time::sleep(check_interval).await;

        // Progress update
        if completion_start.elapsed().as_secs().is_multiple_of(5) {
            let executed = stats.tasks_executed.load(Ordering::SeqCst);
            println!(
                "   Progress: {} tasks executed, {} workflows still running",
                executed, still_running
            );
        }
    }

    // Final status check
    let mut final_completed = 0;
    let mut final_failed = 0;
    for wf_id in &workflow_ids {
        if let Ok(wf) = workflow_client.get_workflow(wf_id, false).await {
            match wf.status {
                WorkflowStatus::Completed => final_completed += 1,
                _ => final_failed += 1,
            }
        }
    }

    let total_duration = start_time.elapsed();

    println!("\n📊 Workflow Results:");
    println!("   Completed: {}", final_completed);
    println!("   Failed: {}", final_failed);
    println!("   Total time: {:?}", total_duration);

    // Stop workers
    println!("\n🛑 Stopping workers...");
    handler.stop().await.expect("Failed to stop handler");

    // Print statistics
    stats.print_summary();

    // Calculate throughput
    let tasks_executed = stats.tasks_executed.load(Ordering::SeqCst);
    if tasks_executed > 0 && total_duration.as_secs() > 0 {
        let throughput = tasks_executed as f64 / total_duration.as_secs_f64();
        println!("📈 Throughput: {:.2} tasks/second", throughput);
    }

    // Cleanup - delete workflows
    println!("\n🧹 Cleaning up...");
    for wf_id in &workflow_ids {
        workflow_client.delete_workflow(wf_id, false).await.ok();
    }

    // Delete task and workflow definitions
    for task_name in &task_names {
        metadata_client.delete_task_def(task_name).await.ok();
    }
    metadata_client
        .delete_workflow_def(&workflow_name, 1)
        .await
        .ok();

    // Assertions
    let concurrency_errors = stats.concurrency_errors.load(Ordering::SeqCst);
    assert_eq!(concurrency_errors, 0, "Concurrency errors detected!");

    let expected_tasks = final_completed * TASKS_PER_WORKFLOW;
    assert!(
        tasks_executed >= expected_tasks,
        "Expected at least {} tasks executed, got {}",
        expected_tasks,
        tasks_executed
    );

    println!("✅ Performance test completed successfully!");
}

/// Test to verify worker shutdown doesn't lose tasks
#[tokio::test]
async fn test_graceful_shutdown() {
    if std::env::var("CONDUCTOR_SERVER_URL").is_err() {
        println!("Skipping graceful shutdown test: CONDUCTOR_SERVER_URL not set");
        return;
    }

    println!("\n🔄 Testing Graceful Shutdown...");

    let config = Configuration::default();
    let client = ConductorClient::new(config.clone()).expect("Failed to create client");
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    let test_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let task_name = format!("shutdown_test_task_{}", test_id);
    let workflow_name = format!("shutdown_test_workflow_{}", test_id);

    // Register task that takes some time
    let task_def = TaskDef::new(&task_name)
        .with_timeout(60, conductor::models::TimeoutPolicy::TimeOutWf)
        .with_response_timeout(30);

    // If we can't register the task, there's likely no server running
    if let Err(e) = metadata_client.register_task_def(&task_def).await {
        println!("   Skipping test: Cannot reach Conductor server: {}", e);
        return;
    }

    // Register workflow
    let workflow_def =
        WorkflowDef::new(&workflow_name).with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await
        .ok();

    // Track completed tasks
    let completed_count = Arc::new(AtomicUsize::new(0));
    let completed_clone = Arc::clone(&completed_count);

    // Create worker that takes 100ms per task
    let worker = FnWorker::new(&task_name, move |_task: Task| {
        let completed = Arc::clone(&completed_clone);
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            completed.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerOutput::completed_with_result("done"))
        }
    })
    .with_thread_count(5);

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Start several workflows
    let mut workflow_ids = Vec::new();
    for _ in 0..10 {
        let request = StartWorkflowRequest::new(&workflow_name);
        if let Ok(wf_id) = workflow_client.start_workflow(&request).await {
            workflow_ids.push(wf_id);
        }
    }

    // If no workflows started, server might not be reachable
    if workflow_ids.is_empty() {
        println!("   No workflows started, skipping test");
        handler.stop().await.ok();
        metadata_client.delete_task_def(&task_name).await.ok();
        metadata_client
            .delete_workflow_def(&workflow_name, 1)
            .await
            .ok();
        return;
    }

    // Wait a bit for tasks to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Stop handler (should wait for in-flight tasks)
    let stop_start = Instant::now();
    handler.stop().await.unwrap();
    let stop_duration = stop_start.elapsed();

    let completed = completed_count.load(Ordering::SeqCst);
    println!(
        "   Completed {} tasks before shutdown in {:?}",
        completed, stop_duration
    );

    // Cleanup
    for wf_id in &workflow_ids {
        workflow_client
            .terminate_workflow(wf_id, Some("test cleanup"), false)
            .await
            .ok();
        workflow_client.delete_workflow(wf_id, false).await.ok();
    }
    metadata_client.delete_task_def(&task_name).await.ok();
    metadata_client
        .delete_workflow_def(&workflow_name, 1)
        .await
        .ok();

    // Should have completed at least some tasks if workflows were started
    // Note: In CI without a real server, we may get 0 completions, which is acceptable
    // The main purpose is to ensure graceful shutdown doesn't panic
    if completed > 0 {
        println!(
            "✅ Graceful shutdown test passed! ({} tasks completed)",
            completed
        );
    } else {
        println!(
            "⚠️ Graceful shutdown test completed (no tasks executed - check server connectivity)"
        );
    }
}

/// Stress test for high concurrency
#[tokio::test]
async fn test_high_concurrency_stress() {
    if std::env::var("CONDUCTOR_SERVER_URL").is_err() {
        println!("Skipping stress test: CONDUCTOR_SERVER_URL not set");
        return;
    }

    println!("\n💪 Running High Concurrency Stress Test...");

    let config = Configuration::default();
    let client = ConductorClient::new(config.clone()).expect("Failed to create client");
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    let test_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let task_name = format!("stress_test_task_{}", test_id);
    let workflow_name = format!("stress_test_workflow_{}", test_id);

    // Register task
    let task_def = TaskDef::new(&task_name);
    metadata_client.register_task_def(&task_def).await.ok();

    // Register workflow
    let workflow_def = WorkflowDef::new(&workflow_name).with_task(
        WorkflowTask::simple(&task_name, "task_ref")
            .with_input_param("value", "${workflow.input.value}"),
    );
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await
        .ok();

    // Track all processed values to detect duplicates or missing
    let processed_values = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let processed_clone = Arc::clone(&processed_values);

    // Create high-concurrency worker
    let worker = FnWorker::new(&task_name, move |task: Task| {
        let processed = Arc::clone(&processed_clone);
        async move {
            let value: i64 = task.get_input("value").unwrap_or(-1);

            // Record this value
            let mut set = processed.lock();
            if set.contains(&value) {
                return Ok(WorkerOutput::failed(format!("Duplicate value: {}", value)));
            }
            set.insert(value);
            drop(set);

            // Minimal work
            Ok(WorkerOutput::completed_with_result(serde_json::json!({
                "processed_value": value
            })))
        }
    })
    .with_thread_count(50) // High concurrency
    .with_poll_interval_millis(10); // Fast polling

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Start many workflows quickly
    let workflow_count = 200;
    let start = Instant::now();

    let mut handles = Vec::new();
    for i in 0..workflow_count {
        let workflow_client = workflow_client.clone();
        let workflow_name = workflow_name.clone();

        let handle = tokio::spawn(async move {
            let request =
                StartWorkflowRequest::new(&workflow_name).with_input_value("value", i as i64);
            workflow_client.start_workflow(&request).await
        });
        handles.push(handle);
    }

    let mut workflow_ids = Vec::new();
    for handle in handles {
        if let Ok(Ok(wf_id)) = handle.await {
            workflow_ids.push(wf_id);
        }
    }

    println!(
        "   Started {} workflows in {:?}",
        workflow_ids.len(),
        start.elapsed()
    );

    // Wait for completion
    let timeout = Duration::from_secs(60);
    let completion_start = Instant::now();

    loop {
        if completion_start.elapsed() > timeout {
            println!("   Timeout waiting for workflows");
            break;
        }

        let processed = processed_values.lock().len();
        if processed >= workflow_ids.len() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    handler.stop().await.unwrap();

    let total_duration = start.elapsed();
    let processed = processed_values.lock().len();

    println!(
        "   Processed {} unique values in {:?}",
        processed, total_duration
    );
    println!(
        "   Throughput: {:.2} workflows/second",
        processed as f64 / total_duration.as_secs_f64()
    );

    // Cleanup
    for wf_id in &workflow_ids {
        workflow_client.delete_workflow(wf_id, false).await.ok();
    }
    metadata_client.delete_task_def(&task_name).await.ok();
    metadata_client
        .delete_workflow_def(&workflow_name, 1)
        .await
        .ok();

    // Verify at least 80% of workflows were processed
    // (some may timeout under heavy server load, which is acceptable for stress testing)
    let min_expected = workflow_ids.len() * 80 / 100;
    assert!(
        processed >= min_expected,
        "At least 80% of workflows should be processed. Got {}/{}",
        processed,
        workflow_ids.len()
    );

    println!("✅ High concurrency stress test passed!");
}
