//! Common test utilities for Conductor Rust SDK tests
//!
//! This module provides shared utilities, helpers, and common test data
//! used across all test files.

use conductor::{
    client::ConductorClient, configuration::Configuration, error::Result, models::WorkflowStatus,
};
use std::time::Duration;

/// Common test constants
#[allow(dead_code)]
pub const TEST_WORKFLOW_NAME: &str = "test-sdk-rust-workflow";
#[allow(dead_code)]
pub const TEST_TASK_NAME: &str = "test-sdk-rust-task";
#[allow(dead_code)]
pub const TEST_OWNER_EMAIL: &str = "test@orkes.io";
#[allow(dead_code)]
pub const TEST_WORKFLOW_VERSION: i32 = 1;

/// Get test configuration from environment
pub fn test_config() -> Configuration {
    Configuration::from_env()
}



/// Generate a unique workflow name with prefix
#[allow(dead_code)]
pub fn generate_unique_workflow_name(prefix: &str) -> String {
    let uuid = uuid::Uuid::new_v4().to_string();
    format!("{}_{}", prefix, &uuid[..8])
}

/// Generate a unique task name with prefix
#[allow(dead_code)]
pub fn generate_unique_task_name(prefix: &str) -> String {
    let uuid = uuid::Uuid::new_v4().to_string();
    format!("{}_{}", prefix, &uuid[..8])
}

/// Generate a unique name (generic)
pub fn generate_unique_name(prefix: &str) -> String {
    let uuid = uuid::Uuid::new_v4().to_string();
    format!("{}_{}", prefix, &uuid[..8])
}

/// Cleanup workflow by ID (best effort - doesn't fail)
#[allow(dead_code)]
pub async fn cleanup_workflow(client: &ConductorClient, workflow_id: &str) {
    let workflow_client = client.workflow_client();
    
    // Try to terminate first
    workflow_client
        .terminate_workflow(workflow_id, Some("test cleanup"), false)
        .await
        .ok();
    
    // Then delete
    workflow_client
        .delete_workflow(workflow_id, false)
        .await
        .ok();
}

/// Cleanup task definition (best effort - doesn't fail)
#[allow(dead_code)]
pub async fn cleanup_task_def(client: &ConductorClient, task_name: &str) {
    client
        .metadata_client()
        .delete_task_def(task_name)
        .await
        .ok();
}

/// Cleanup workflow definition (best effort - doesn't fail)
#[allow(dead_code)]
pub async fn cleanup_workflow_def(client: &ConductorClient, name: &str, version: i32) {
    client
        .metadata_client()
        .delete_workflow_def(name, version)
        .await
        .ok();
}

/// Retry a function with exponential backoff for eventual consistency
///
/// This is useful for operations that may take time to reflect in the system.
pub async fn retry_with_backoff<F, Fut, T>(
    func: F,
    max_retries: u32,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(100);

    loop {
        match func().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                retries += 1;
                if retries >= max_retries {
                    return Err(e);
                }
                tokio::time::sleep(delay).await;
                delay = delay.saturating_mul(2); // Exponential backoff
            }
        }
    }
}

/// Wait for workflow to reach a specific status
#[allow(dead_code)]
pub async fn wait_for_workflow_status(
    client: &ConductorClient,
    workflow_id: &str,
    expected_status: WorkflowStatus,
    timeout: Duration,
) -> Result<()> {
    let start = std::time::Instant::now();
    let workflow_client = client.workflow_client();

    loop {
        if start.elapsed() > timeout {
            return Err(conductor::error::ConductorError::Timeout(format!(
                "Workflow {} did not reach status {:?} within timeout",
                workflow_id, expected_status
            )));
        }

        let workflow = workflow_client.get_workflow(workflow_id, false).await?;
        if workflow.status == expected_status {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Wait for workflow to complete (success or failure)
#[allow(dead_code)]
pub async fn wait_for_workflow_completion(
    client: &ConductorClient,
    workflow_id: &str,
    timeout: Duration,
) -> Result<WorkflowStatus> {
    let start = std::time::Instant::now();
    let workflow_client = client.workflow_client();

    loop {
        if start.elapsed() > timeout {
            return Err(conductor::error::ConductorError::Timeout(format!(
                "Workflow {} did not complete within timeout",
                workflow_id
            )));
        }

        let workflow = workflow_client.get_workflow(workflow_id, false).await?;
        
        // Check if workflow is in a terminal state
        match workflow.status {
            WorkflowStatus::Completed
            | WorkflowStatus::Failed
            | WorkflowStatus::Terminated
            | WorkflowStatus::TimedOut => {
                return Ok(workflow.status);
            }
            _ => {}
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Create a simple worker that returns a fixed result
#[macro_export]
macro_rules! simple_worker {
    ($task_name:expr, $result:expr) => {
        conductor::worker::FnWorker::new($task_name.clone(), move |_task| async move {
            Ok(conductor::worker::WorkerOutput::completed_with_result(
                $result,
            ))
        })
    };
}

/// Create a worker that fails
#[macro_export]
macro_rules! failing_worker {
    ($task_name:expr, $error_msg:expr) => {
        conductor::worker::FnWorker::new($task_name.clone(), move |_task| async move {
            Ok(conductor::worker::WorkerOutput::failed($error_msg))
        })
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_unique_name() {
        let name1 = generate_unique_name("test");
        let name2 = generate_unique_name("test");
        
        assert!(name1.starts_with("test_"));
        assert!(name2.starts_with("test_"));
        assert_ne!(name1, name2); // Should be unique
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success() {
        use std::cell::Cell;
        let attempt = Cell::new(0);
        let result = retry_with_backoff(
            || async {
                let current = attempt.get();
                attempt.set(current + 1);
                if current < 2 {
                    Err(conductor::error::ConductorError::Internal(
                        "not yet".to_string(),
                    ))
                } else {
                    Ok(42)
                }
            },
            5,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_failure() {
        let result = retry_with_backoff(
            || async {
                Err::<i32, _>(conductor::error::ConductorError::Internal(
                    "always fails".to_string(),
                ))
            },
            3,
        )
        .await;

        assert!(result.is_err());
    }
}
