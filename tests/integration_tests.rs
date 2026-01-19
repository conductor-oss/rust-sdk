//! Integration tests for Conductor Rust SDK
//!
//! These tests require a running Conductor server on localhost:8080

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{
        RetryLogic, StartWorkflowRequest, Task, TaskDef, TimeoutPolicy, WorkflowDef, WorkflowTask,
    },
    worker::{FnWorker, TaskHandler, WorkerOutput},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Test configuration from environment
fn test_config() -> Configuration {
    Configuration::from_env()
}

/// Check if Conductor server is available
async fn conductor_available() -> bool {
    let config = test_config();
    let client = match ConductorClient::new(config) {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Try to get metadata to verify connection
    client.metadata_client().get_all_task_defs().await.is_ok()
}

#[tokio::test]
async fn test_task_def_crud() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();

    // Create a unique task name
    let task_name = format!("test_task_{}", uuid::Uuid::new_v4());

    // Create task definition
    let task_def = TaskDef::new(&task_name)
        .with_description("Test task for integration testing")
        .with_retry(3, RetryLogic::LinearBackoff, 5)
        .with_timeout(120, TimeoutPolicy::Retry)
        .with_rate_limit(100, 10);

    // Register
    metadata.register_task_def(&task_def).await.unwrap();

    // Get and verify
    let retrieved = metadata.get_task_def(&task_name).await.unwrap();
    assert_eq!(retrieved.name, task_name);
    assert_eq!(retrieved.retry_count, 3);
    assert_eq!(retrieved.retry_logic, RetryLogic::LinearBackoff);

    // Update
    let updated_def = TaskDef::new(&task_name).with_retry(5, RetryLogic::ExponentialBackoff, 10);
    metadata.update_task_def(&updated_def).await.unwrap();

    // Verify update
    let retrieved = metadata.get_task_def(&task_name).await.unwrap();
    assert_eq!(retrieved.retry_count, 5);
    assert_eq!(retrieved.retry_logic, RetryLogic::ExponentialBackoff);

    // Delete
    metadata.delete_task_def(&task_name).await.unwrap();

    // Verify deleted
    let exists = metadata.task_def_exists(&task_name).await.unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_workflow_def_crud() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();

    // Create a unique workflow name
    let workflow_name = format!("test_workflow_{}", uuid::Uuid::new_v4());

    // Create workflow definition
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_description("Test workflow for integration testing")
        .with_version(1)
        .with_task(WorkflowTask::simple("simple_task", "simple_task_ref"));

    // Register
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Get and verify
    let retrieved = metadata
        .get_workflow_def(&workflow_name, Some(1))
        .await
        .unwrap();
    assert_eq!(retrieved.name, workflow_name);
    assert_eq!(retrieved.version, 1);

    // Check exists
    let exists = metadata
        .workflow_def_exists(&workflow_name, Some(1))
        .await
        .unwrap();
    assert!(exists);

    // Delete
    metadata
        .delete_workflow_def(&workflow_name, 1)
        .await
        .unwrap();

    // Verify deleted
    let exists = metadata
        .workflow_def_exists(&workflow_name, Some(1))
        .await
        .unwrap();
    assert!(!exists);
}

#[tokio::test]
async fn test_workflow_execution() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    // Create unique names
    let task_name = format!("test_exec_task_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let workflow_name = format!(
        "test_exec_workflow_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    // Register task definition
    let task_def = TaskDef::new(&task_name).with_timeout(60, TimeoutPolicy::TimeOutWf);
    metadata.register_task_def(&task_def).await.unwrap();

    // Register workflow definition
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(
            WorkflowTask::simple(&task_name, "task_ref")
                .with_input_param("input", "${workflow.input.input}"),
        )
        .with_output_param("result", "${task_ref.output.result}");
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create worker
    let execution_count = Arc::new(AtomicUsize::new(0));
    let exec_count_clone = execution_count.clone();

    let worker = FnWorker::new(task_name.clone(), move |task: Task| {
        let count = exec_count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            let input = task.get_input_string("input").unwrap_or_default();
            Ok(WorkerOutput::completed_with_result(format!(
                "processed: {}",
                input
            )))
        }
    });

    // Start handler
    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name)
        .with_version(1)
        .with_input_value("input", "test_value");

    let workflow_run = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    // Verify execution
    assert!(
        workflow_run.is_successful(),
        "Workflow should complete successfully"
    );
    assert_eq!(
        execution_count.load(Ordering::SeqCst),
        1,
        "Worker should execute once"
    );

    // Stop handler
    handler.stop().await.unwrap();

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
    metadata.delete_task_def(&task_name).await.ok();
}

#[tokio::test]
async fn test_workflow_lifecycle_operations() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    // Create unique names
    let task_name = format!(
        "test_lifecycle_task_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let workflow_name = format!(
        "test_lifecycle_wf_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    // Register task with long timeout for testing lifecycle operations
    let task_def = TaskDef::new(&task_name).with_timeout(300, TimeoutPolicy::TimeOutWf);
    metadata.register_task_def(&task_def).await.unwrap();

    // Register workflow
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Start workflow (async - don't wait)
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_id = workflow_client.start_workflow(&request).await.unwrap();

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Test pause
    workflow_client.pause_workflow(&workflow_id).await.unwrap();
    let wf = workflow_client
        .get_workflow(&workflow_id, false)
        .await
        .unwrap();
    assert_eq!(wf.status, conductor::models::WorkflowStatus::Paused);

    // Test resume
    workflow_client.resume_workflow(&workflow_id).await.unwrap();
    let wf = workflow_client
        .get_workflow(&workflow_id, false)
        .await
        .unwrap();
    assert_eq!(wf.status, conductor::models::WorkflowStatus::Running);

    // Test terminate
    workflow_client
        .terminate_workflow(&workflow_id, Some("Test termination"), false)
        .await
        .unwrap();
    let wf = workflow_client
        .get_workflow(&workflow_id, false)
        .await
        .unwrap();
    assert_eq!(wf.status, conductor::models::WorkflowStatus::Terminated);

    // Cleanup
    workflow_client
        .delete_workflow(&workflow_id, false)
        .await
        .ok();
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
    metadata.delete_task_def(&task_name).await.ok();
}

#[tokio::test]
async fn test_multiple_workers() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    // Create unique names
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let task1_name = format!("test_multi_task1_{}", id);
    let task2_name = format!("test_multi_task2_{}", id);
    let workflow_name = format!("test_multi_wf_{}", id);

    // Register tasks
    metadata
        .register_task_def(&TaskDef::new(&task1_name))
        .await
        .unwrap();
    metadata
        .register_task_def(&TaskDef::new(&task2_name))
        .await
        .unwrap();

    // Register workflow with two tasks
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task1_name, "task1_ref"))
        .with_task(
            WorkflowTask::simple(&task2_name, "task2_ref")
                .with_input_param("prev_result", "${task1_ref.output.result}"),
        )
        .with_output_param("final_result", "${task2_ref.output.result}");
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create workers
    let task1_count = Arc::new(AtomicUsize::new(0));
    let task2_count = Arc::new(AtomicUsize::new(0));

    let count1 = task1_count.clone();
    let worker1 = FnWorker::new(task1_name.clone(), move |_: Task| {
        let c = count1.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerOutput::completed_with_result("result_from_task1"))
        }
    });

    let count2 = task2_count.clone();
    let worker2 = FnWorker::new(task2_name.clone(), move |task: Task| {
        let c = count2.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            let prev = task.get_input_string("prev_result").unwrap_or_default();
            Ok(WorkerOutput::completed_with_result(format!(
                "task2_got: {}",
                prev
            )))
        }
    });

    // Start handler with both workers
    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker1);
    handler.add_worker(worker2);
    handler.start().await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_run = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    // Verify
    assert!(workflow_run.is_successful());
    assert_eq!(task1_count.load(Ordering::SeqCst), 1);
    assert_eq!(task2_count.load(Ordering::SeqCst), 1);

    // Stop handler
    handler.stop().await.unwrap();

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
    metadata.delete_task_def(&task1_name).await.ok();
    metadata.delete_task_def(&task2_name).await.ok();
}

// ============================================================================
// HTTP Task Tests
// ============================================================================

#[tokio::test]
async fn test_http_task_workflow() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!("test_http_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Create workflow with HTTP task
    let http_task = WorkflowTask::http("http_ref", "https://httpbin.org/get");

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(http_task)
        .with_output_param("response", "${http_ref.output}");

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "HTTP workflow should complete");
    assert!(result.output.contains_key("response"));

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// ============================================================================
// Inline JavaScript Task Tests
// ============================================================================

#[tokio::test]
async fn test_inline_javascript_task() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!("test_js_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Create workflow with inline JS task
    let script = r#"
    (function() {
        return {
            sum: $.a + $.b,
            message: "Calculated from JS"
        };
    })();
    "#;

    let js_task = WorkflowTask::inline("js_ref", script)
        .with_input_param("a", 10)
        .with_input_param("b", 20);

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(js_task)
        .with_output_param("result", "${js_ref.output.result}");

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "JS workflow should complete");

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// ============================================================================
// Fork/Join Task Tests
// ============================================================================

#[tokio::test]
async fn test_fork_join_workflow() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!("test_fork_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Create workflow with fork/join
    let branch1 = WorkflowTask::inline("branch1_ref", "(function() { return { value: 1 }; })();");
    let branch2 = WorkflowTask::inline("branch2_ref", "(function() { return { value: 2 }; })();");

    let fork = WorkflowTask::fork("fork_ref", vec![vec![branch1], vec![branch2]]);
    let join = WorkflowTask::join(
        "join_ref",
        vec!["branch1_ref".to_string(), "branch2_ref".to_string()],
    );

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(fork)
        .with_task(join);

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "Fork/Join workflow should complete");

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// ============================================================================
// Switch Task Tests
// ============================================================================

#[tokio::test]
async fn test_switch_task_workflow() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!("test_switch_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Create workflow with switch task
    // Use value expression format for simple value matching
    let case_a = WorkflowTask::inline("case_a_ref", "(function() { return { result: 'A' }; })();");
    let case_b = WorkflowTask::inline("case_b_ref", "(function() { return { result: 'B' }; })();");
    let default_case = WorkflowTask::inline(
        "default_ref",
        "(function() { return { result: 'default' }; })();",
    );

    // Use value_param evaluator for simple ${...} expressions
    let switch = WorkflowTask::switch_value_param("switch_ref", "${workflow.input.choice}")
        .with_switch_case("A", vec![case_a])
        .with_switch_case("B", vec![case_b])
        .with_default_case(vec![default_case]);

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(switch);

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow with choice = A
    let request = StartWorkflowRequest::new(&workflow_name)
        .with_version(1)
        .with_input_value("choice", "A");

    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "Switch workflow should complete");

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// ============================================================================
// Do-While Loop Tests
// ============================================================================

#[tokio::test]
async fn test_do_while_workflow() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!("test_dowhile_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Create workflow with do-while loop (3 iterations)
    let loop_task = WorkflowTask::inline(
        "loop_body_ref",
        "(function() { return { iteration: $.iteration || 0 }; })();",
    );

    let do_while = WorkflowTask::do_while(
        "loop_ref",
        "if ($.loop_body_ref['iteration'] < 3) { true; } else { false; }",
        vec![loop_task],
    );

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(do_while);

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "Do-while workflow should complete");

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// ============================================================================
// Workflow Retry Tests
// ============================================================================

#[tokio::test]
async fn test_workflow_retry() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config.clone()).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let task_name = format!("test_retry_task_{}", id);
    let workflow_name = format!("test_retry_wf_{}", id);

    // Register task that will fail initially
    let task_def = TaskDef::new(&task_name).with_retry(0, RetryLogic::Fixed, 0);
    metadata.register_task_def(&task_def).await.unwrap();

    // Create workflow
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple(&task_name, "fail_task_ref"));
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Create worker that fails first, then succeeds
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let count = attempt_count.clone();

    let worker = FnWorker::new(task_name.clone(), move |_: Task| {
        let c = count.clone();
        async move {
            let attempts = c.fetch_add(1, Ordering::SeqCst);
            if attempts == 0 {
                Ok(WorkerOutput::failed("First attempt fails"))
            } else {
                Ok(WorkerOutput::completed_with_result("Success on retry"))
            }
        }
    });

    let mut handler = TaskHandler::new(config.clone()).unwrap();
    handler.add_worker(worker);
    handler.start().await.unwrap();

    // Start workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let workflow_id = workflow_client.start_workflow(&request).await.unwrap();

    // Wait for workflow to fail
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Retry workflow
    workflow_client
        .retry_workflow(&workflow_id, false)
        .await
        .unwrap();

    // Wait for completion
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check status
    let wf = workflow_client
        .get_workflow(&workflow_id, false)
        .await
        .unwrap();
    assert!(
        wf.is_successful() || wf.status == conductor::models::WorkflowStatus::Running,
        "Workflow should be running or completed after retry"
    );

    // Cleanup
    handler.stop().await.unwrap();
    workflow_client
        .terminate_workflow(&workflow_id, Some("cleanup"), false)
        .await
        .ok();
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
    metadata.delete_task_def(&task_name).await.ok();
}

// ============================================================================
// Workflow Search Tests
// ============================================================================

#[tokio::test]
async fn test_workflow_search() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let workflow_client = client.workflow_client();

    // Search for workflows (should not fail even if empty)
    let result = workflow_client
        .search_workflows(None, None, 0, 10)
        .await
        .unwrap();

    // Just verify the call doesn't fail
    assert!(result.total_hits >= 0);
}

// ============================================================================
// HTTP Poll Task Tests (if available)
// ============================================================================

#[tokio::test]
async fn test_http_poll_task() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let workflow_client = client.workflow_client();

    let workflow_name = format!(
        "test_http_poll_wf_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );

    // Create workflow with HTTP Poll task
    let http_poll = WorkflowTask::http_poll("http_poll_ref", "https://httpbin.org/json")
        .with_polling_strategy("FIXED")
        .with_polling_interval(1000)
        // Terminate immediately after first successful response
        .with_termination_condition(
            "(function(){ return $.output.response.statusCode == 200; })();",
        );

    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(http_poll);

    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Execute workflow
    let request = StartWorkflowRequest::new(&workflow_name).with_version(1);
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await
        .unwrap();

    assert!(result.is_successful(), "HTTP Poll workflow should complete");

    // Cleanup
    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}

// =============================================================================
// Additional Metadata Client Tests
// =============================================================================

#[tokio::test]
async fn test_get_all_workflows_with_latest_versions() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();

    // Get all workflows
    let _workflows = metadata.get_all_workflow_defs_latest_versions().await.unwrap();
    
    // Should return a list (may be empty on fresh install, but should not error)
    // workflows is a Vec, may be empty on fresh install but call should succeed
}

#[tokio::test]
async fn test_get_all_task_defs() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();

    // Get all task defs
    let _tasks = metadata.get_all_task_defs().await.unwrap();
    
    // Should return a list (may be empty on fresh install, but should not error)
    // tasks is a Vec, may be empty on fresh install but call should succeed
}

#[tokio::test]
async fn test_task_tagging() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let orkes_metadata = client.orkes_metadata_client();

    let task_name = format!("test_tag_task_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Register task (base MetadataClient)
    let task_def = TaskDef::new(&task_name)
        .with_description("Task for tag testing");
    metadata.register_task_def(&task_def).await.unwrap();

    // Add a tag (OrkesMetadataClient)
    let tag = conductor::models::MetadataTag::with_value("environment", "test");
    
    match orkes_metadata.add_task_tag(&task_name, &tag).await {
        Ok(_) => {
            // Get tags
            match orkes_metadata.get_task_tags(&task_name).await {
                Ok(tags) => {
                    assert!(!tags.is_empty(), "Should have at least one tag");
                    assert!(tags.iter().any(|t| t.key == "environment"));
                }
                Err(e) => eprintln!("Warning: get_task_tags failed: {:?}", e),
            }
            
            // Delete tag
            orkes_metadata.delete_task_tag(&task_name, &tag).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: add_task_tag failed (may require Orkes): {:?}", e);
        }
    }

    // Cleanup (base MetadataClient via Deref)
    orkes_metadata.delete_task_def(&task_name).await.ok();
}

#[tokio::test]
async fn test_workflow_tagging() {
    if !conductor_available().await {
        eprintln!("Skipping test: Conductor server not available");
        return;
    }

    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let metadata = client.metadata_client();
    let orkes_metadata = client.orkes_metadata_client();

    let workflow_name = format!("test_tag_wf_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    // Register workflow (base MetadataClient)
    let workflow_def = WorkflowDef::new(&workflow_name)
        .with_version(1)
        .with_task(WorkflowTask::simple("simple_task", "simple_ref"));
    
    metadata.register_workflow_def(&workflow_def).await.unwrap();

    // Add a tag (OrkesMetadataClient)
    let tag = conductor::models::MetadataTag::with_value("team", "platform");
    
    match orkes_metadata.add_workflow_tag(&workflow_name, &tag).await {
        Ok(_) => {
            // Get tags
            match orkes_metadata.get_workflow_tags(&workflow_name).await {
                Ok(tags) => {
                    assert!(!tags.is_empty(), "Should have at least one tag");
                    assert!(tags.iter().any(|t| t.key == "team"));
                }
                Err(e) => eprintln!("Warning: get_workflow_tags failed: {:?}", e),
            }
            
            // Delete tag
            orkes_metadata.delete_workflow_tag(&workflow_name, &tag).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: add_workflow_tag failed (may require Orkes): {:?}", e);
        }
    }

    // Cleanup (base MetadataClient via Deref)
    orkes_metadata.delete_workflow_def(&workflow_name, 1).await.ok();
}
