// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::time::Duration;
use tracing::{debug, warn};

use crate::error::{ConductorError, Result};
use crate::http::{ApiClient, ApiPath};
use crate::models::{Task, TaskResult};

/// Client for task operations (polling and updates)
#[derive(Clone)]
pub struct TaskClient {
    api: ApiClient,
}

impl TaskClient {
    /// Create a new task client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Poll for a single task
    pub async fn poll_task(
        &self,
        task_type: &str,
        worker_id: Option<&str>,
        domain: Option<&str>,
    ) -> Result<Option<Task>> {
        let mut params = vec![];

        if let Some(wid) = worker_id {
            params.push(("workerid", wid));
        }
        if let Some(d) = domain {
            params.push(("domain", d));
        }

        let path = format!("/tasks/poll/{}", task_type);
        let result: Option<Task> = self
            .api
            .get_with_params(ApiPath::templated(&path, "/tasks/poll/{taskType}"), &params)
            .await?;
        Ok(result)
    }

    /// Batch poll for multiple tasks
    pub async fn batch_poll(
        &self,
        task_type: &str,
        worker_id: Option<&str>,
        domain: Option<&str>,
        count: usize,
        timeout: Duration,
    ) -> Result<Vec<Task>> {
        let count_str = count.to_string();
        let timeout_str = timeout.as_millis().to_string();

        let mut params = vec![
            ("count", count_str.as_str()),
            ("timeout", timeout_str.as_str()),
        ];

        if let Some(wid) = worker_id {
            params.push(("workerid", wid));
        }
        if let Some(d) = domain {
            params.push(("domain", d));
        }

        let path = format!("/tasks/poll/batch/{}", task_type);

        debug!(
            task_type = task_type,
            count = count,
            timeout_ms = timeout.as_millis(),
            "Batch polling tasks"
        );

        let tasks: Vec<Task> = self
            .api
            .get_with_params(ApiPath::templated(&path, "/tasks/poll/batch/{taskType}"), &params)
            .await?;

        debug!(
            task_type = task_type,
            tasks_received = tasks.len(),
            "Batch poll completed"
        );

        Ok(tasks)
    }

    /// Update a task result
    pub async fn update_task(&self, result: &TaskResult) -> Result<String> {
        debug!(
            task_id = %result.task_id,
            status = ?result.status,
            "Updating task"
        );

        let response: String = self.api.post_text("/tasks", result).await?;
        Ok(response)
    }

    /// Update task with retry logic
    pub async fn update_task_with_retry(
        &self,
        result: &TaskResult,
        max_attempts: u32,
    ) -> Result<String> {
        let delays = [
            Duration::from_secs(10),
            Duration::from_secs(20),
            Duration::from_secs(30),
        ];

        let mut last_error = None;

        for attempt in 0..max_attempts {
            match self.update_task(result).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if !e.is_retryable() {
                        return Err(e);
                    }

                    last_error = Some(e);

                    if attempt < max_attempts - 1 {
                        let delay = delays
                            .get(attempt as usize)
                            .copied()
                            .unwrap_or(Duration::from_secs(30));

                        warn!(
                            task_id = %result.task_id,
                            attempt = attempt + 1,
                            delay_secs = delay.as_secs(),
                            "Task update failed, retrying"
                        );

                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| ConductorError::internal("Update failed with no error")))
    }

    /// Get task by ID
    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let path = format!("/tasks/{}", task_id);
        self.api.get(ApiPath::templated(&path, "/tasks/{taskId}")).await
    }

    /// Get tasks in progress for a task type
    pub async fn get_tasks_in_progress(
        &self,
        task_type: &str,
        start_key: Option<&str>,
        count: Option<i32>,
    ) -> Result<Vec<Task>> {
        let mut params = vec![];
        let count_str;

        if let Some(key) = start_key {
            params.push(("startKey", key));
        }
        if let Some(c) = count {
            count_str = c.to_string();
            params.push(("count", &count_str));
        }

        let path = format!("/tasks/in_progress/{}", task_type);
        self.api
            .get_with_params(ApiPath::templated(&path, "/tasks/in_progress/{taskType}"), &params)
            .await
    }

    /// Add a log to a task
    pub async fn add_task_log(&self, task_id: &str, log: &str) -> Result<()> {
        let path = format!("/tasks/{}/log", task_id);
        let _: serde_json::Value = self.api.post(ApiPath::templated(&path, "/tasks/{taskId}/log"), &log).await?;
        Ok(())
    }

    /// Get logs for a task
    pub async fn get_task_logs(
        &self,
        task_id: &str,
    ) -> Result<Vec<crate::models::task::TaskExecLog>> {
        let path = format!("/tasks/{}/log", task_id);
        self.api.get(ApiPath::templated(&path, "/tasks/{taskId}/log")).await
    }

    /// Get task queue sizes
    pub async fn get_queue_sizes(
        &self,
        task_types: &[&str],
    ) -> Result<std::collections::HashMap<String, i64>> {
        let params: Vec<(&str, &str)> = task_types.iter().map(|t| ("taskType", *t)).collect();
        self.api.get_with_params("/tasks/queue/sizes", &params).await
    }

    /// Remove task from queue
    pub async fn remove_task_from_queue(&self, task_type: &str, task_id: &str) -> Result<()> {
        let path = format!("/tasks/queue/{}/{}", task_type, task_id);
        self.api
            .delete_no_content(ApiPath::templated(&path, "/tasks/queue/{taskType}/{taskId}"))
            .await
    }

    /// Update task by reference name
    pub async fn update_task_by_ref_name(
        &self,
        workflow_id: &str,
        task_ref_name: &str,
        status: crate::models::TaskResultStatus,
        output: serde_json::Value,
        worker_id: Option<&str>,
    ) -> Result<String> {
        let path = format!(
            "/tasks/{}/{}/{}",
            workflow_id,
            task_ref_name,
            status_to_string(&status)
        );
        let mut params = vec![];
        if let Some(wid) = worker_id {
            params.push(("workerid", wid));
        }

        // POST with output as body
        self.api
            .post_text(
                ApiPath::templated(&path, "/tasks/{workflowId}/{taskRefName}/{status}"),
                &output,
            )
            .await
    }

    /// Update task synchronously and return the updated workflow
    pub async fn update_task_sync(
        &self,
        workflow_id: &str,
        task_ref_name: &str,
        status: crate::models::TaskResultStatus,
        output: serde_json::Value,
        worker_id: Option<&str>,
    ) -> Result<crate::models::Workflow> {
        let path = format!(
            "/tasks/{}/{}/{}/sync",
            workflow_id,
            task_ref_name,
            status_to_string(&status)
        );
        let mut params: Vec<(&str, String)> = vec![];
        if let Some(wid) = worker_id {
            params.push(("workerid", wid.to_string()));
        }

        let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api
            .post_with_params(
                ApiPath::templated(&path, "/tasks/{workflowId}/{taskRefName}/{status}/sync"),
                &output,
                &params_ref,
            )
            .await
    }

    /// Get queue size for a specific task type
    pub async fn get_queue_size_for_task(&self, task_type: &str) -> Result<i64> {
        let sizes = self.get_queue_sizes(&[task_type]).await?;
        Ok(*sizes.get(task_type).unwrap_or(&0))
    }

    /// Get poll data for a task type
    pub async fn get_task_poll_data(&self, task_type: &str) -> Result<Vec<PollData>> {
        let path = format!("/tasks/queue/polldata/{}", task_type);
        self.api
            .get(ApiPath::templated(&path, "/tasks/queue/polldata/{taskType}"))
            .await
    }

    /// Get all poll data
    pub async fn get_all_poll_data(&self) -> Result<Vec<PollData>> {
        self.api.get("/tasks/queue/polldata/all").await
    }

    /// Get poll data (alias for get_task_poll_data)
    pub async fn get_poll_data(&self, task_type: &str) -> Result<Vec<PollData>> {
        self.get_task_poll_data(task_type).await
    }

    /// Search for tasks
    pub async fn search_tasks(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<crate::client::workflow_client::SearchResult<Task>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_string()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_string()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api
            .get_with_params("/tasks/search", &params)
            .await
    }

    /// Search for tasks V2 (returns full task objects)
    pub async fn search_tasks_v2(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<crate::client::workflow_client::SearchResult<Task>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_string()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_string()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api
            .get_with_params("/tasks/search-v2", &params)
            .await
    }

    /// Requeue pending tasks
    pub async fn requeue_pending_tasks(&self, task_type: &str) -> Result<String> {
        let path = format!("/tasks/queue/requeue/{}", task_type);
        self.api
            .post_text(
                ApiPath::templated(&path, "/tasks/queue/requeue/{taskType}"),
                &serde_json::Value::Null,
            )
            .await
    }
}

/// Poll data for a task queue
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollData {
    /// Queue name / task type
    #[serde(default)]
    pub queue_name: String,

    /// Domain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Worker ID that last polled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,

    /// Last poll time
    #[serde(default)]
    pub last_poll_time: i64,
}

fn status_to_string(status: &crate::models::TaskResultStatus) -> &'static str {
    match status {
        crate::models::TaskResultStatus::Completed => "COMPLETED",
        crate::models::TaskResultStatus::Failed => "FAILED",
        crate::models::TaskResultStatus::FailedWithTerminalError => "FAILED_WITH_TERMINAL_ERROR",
        crate::models::TaskResultStatus::InProgress => "IN_PROGRESS",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskResultStatus;

    #[test]
    fn test_task_result_creation() {
        let result = TaskResult::completed("task-1", "wf-1")
            .with_worker_id("worker-1")
            .with_output_value("result", "success");

        assert_eq!(result.task_id, "task-1");
        assert_eq!(result.workflow_instance_id, "wf-1");
        assert_eq!(result.status, TaskResultStatus::Completed);
    }
}
