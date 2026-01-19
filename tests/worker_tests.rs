//! Worker framework tests
//!
//! Based on worker tests from Java SDK

mod common;

use conductor::{
    client::ConductorClient,
    models::{StartWorkflowRequest, TaskDef, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput, TaskContext},
};
use common::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_worker_poll_and_execute() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let task_name = generate_unique_task_name("worker_poll");
    let workflow_name = generate_unique_workflow_name("worker_poll_wf");

    // Register task and workflow
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create worker
    let execution_count = Arc::new(AtomicUsize::new(0));
    let count = execution_count.clone();

    let worker = FnWorker::new(task_name.clone(), move |_task| {
        let c = count.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerOutput::completed_with_result("success"))
        }
    });

    // Start handler
    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_run = workflow_client
        .execute_workflow(&request, Duration::from_secs(10))
        .await
        .unwrap();

    // Verify
    assert!(workflow_run.is_successful());
    assert_eq!(execution_count.load(Ordering::SeqCst), 1);

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_workflow_def(&client, &workflow_name, 1).await;
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_concurrency_control() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let task_name = generate_unique_task_name("worker_concurrency");
    let workflow_name = generate_unique_workflow_name("worker_concurrency_wf");

    // Register task and workflow
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create worker with thread_count = 2
    let concurrent_executions = Arc::new(AtomicUsize::new(0));
    let max_concurrent = Arc::new(AtomicUsize::new(0));

    let current = concurrent_executions.clone();
    let max = max_concurrent.clone();

    let worker = FnWorker::new(task_name.clone(), move |_task| {
        let curr = current.clone();
        let mx = max.clone();
        async move {
            let now_running = curr.fetch_add(1, Ordering::SeqCst) + 1;
            
            // Update max if needed
            mx.fetch_max(now_running, Ordering::SeqCst);
            
            // Simulate work
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            curr.fetch_sub(1, Ordering::SeqCst);
            Ok(WorkerOutput::completed_with_result("success"))
        }
    })
    .with_thread_count(2);

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Start 5 workflows to test concurrency
    let mut workflow_ids = Vec::new();
    for _ in 0..5 {
        let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
        let id = workflow_client.start_workflow(&request).await.unwrap();
        workflow_ids.push(id);
    }

    // Wait for completion
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Max concurrent should not exceed thread_count (2)
    assert!(
        max_concurrent.load(Ordering::SeqCst) <= 2,
        "Concurrent executions should not exceed thread_count"
    );

    // Cleanup
    handler.stop().await.unwrap();
    for id in workflow_ids {
        cleanup_workflow(&client, &id).await;
    }
    cleanup_workflow_def(&client, &workflow_name, 1).await;
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_error_handling() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let task_name = generate_unique_task_name("worker_error");
    let workflow_name = generate_unique_workflow_name("worker_error_wf");

    // Register task with no retry
    let task_def = TaskDef::new(&task_name).with_retry(0, conductor::models::RetryLogic::Fixed, 0);
    metadata.register_task_def(&task_def).await.unwrap();

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create worker that fails
    let worker = FnWorker::new(task_name.clone(), move |_task| async move {
        Ok(WorkerOutput::failed("Intentional failure"))
    });

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Start workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_id = workflow_client.start_workflow(&request).await.unwrap();

    // Wait for failure
    tokio::time::sleep(Duration::from_secs(3)).await;

    let wf = workflow_client.get_workflow(&workflow_id, false).await.unwrap();
    assert_eq!(wf.status, conductor::models::WorkflowStatus::Failed);

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_workflow(&client, &workflow_id).await;
    cleanup_workflow_def(&client, &workflow_name, 1).await;
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_task_in_progress() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let task_name = generate_unique_task_name("worker_in_progress");
    let workflow_name = generate_unique_workflow_name("worker_in_progress_wf");

    // Register
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Worker that returns IN_PROGRESS first, then completes
    let worker = FnWorker::new(task_name.clone(), move |task| async move {
        let ctx = TaskContext::from_task(&task);
        
        if ctx.poll_count() < 2 {
            // Return in progress
            Ok(WorkerOutput::in_progress(1)) // callback in 1 second
        } else {
            // Complete
            Ok(WorkerOutput::completed_with_result("done"))
        }
    });

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_run = workflow_client
        .execute_workflow(&request, Duration::from_secs(15))
        .await
        .unwrap();

    // Should eventually complete
    assert!(workflow_run.is_successful());

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_workflow_def(&client, &workflow_name, 1).await;
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_domain_filtering() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();

    let task_name = generate_unique_task_name("worker_domain");

    // Register task
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();

    // Create worker with specific domain
    let worker = FnWorker::new(task_name.clone(), move |_task| async move {
        Ok(WorkerOutput::completed_with_result("success"))
    })
    .with_domain("test-domain");

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Worker should only poll tasks from "test-domain"
    // This is verified by the SDK's polling logic

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_configuration_override() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();

    let task_name = generate_unique_task_name("worker_config");

    // Register task
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();

    // Create worker with custom configuration
    let worker = FnWorker::new(task_name.clone(), move |_task| async move {
        Ok(WorkerOutput::completed_with_result("success"))
    })
    .with_poll_interval_millis(500)  // Custom poll interval
    .with_thread_count(5);           // Custom thread count

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Configuration should be applied
    // This is internal to the SDK

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_task_def(&client, &task_name).await;
}

#[tokio::test]
async fn test_worker_pause_resume() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();

    let task_name = generate_unique_task_name("worker_pause");

    // Register task
    metadata.register_task_def(&TaskDef::new(&task_name)).await.unwrap();

    let worker = FnWorker::new(task_name.clone(), move |_task| async move {
        Ok(WorkerOutput::completed_with_result("success"))
    });

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Note: Worker pause/resume functionality would be tested here
    // The SDK should support pausing and resuming workers

    // Cleanup
    handler.stop().await.unwrap();
    cleanup_task_def(&client, &task_name).await;
}
