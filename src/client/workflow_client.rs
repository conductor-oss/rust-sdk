// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info};

use crate::error::Result;
use crate::events::{exception_label, EventDispatcher, WorkflowStartFailure, WorkflowStarted};
use crate::http::ApiClient;
use crate::models::{StartWorkflowRequest, Workflow, WorkflowDef};

/// Client for workflow operations
#[derive(Clone)]
pub struct WorkflowClient {
    api: ApiClient,
    /// Event dispatcher used to publish `WorkflowStarted` /
    /// `WorkflowStartFailure`. Defaults to an empty dispatcher (no-op);
    /// construct via [`WorkflowClient::new_with_events`] to hook metrics in.
    events: EventDispatcher,
}

impl WorkflowClient {
    /// Create a new workflow client without an event dispatcher.
    ///
    /// The `WorkflowStarted` / `WorkflowStartFailure` events will still be
    /// published, but no listeners will see them. Use
    /// [`WorkflowClient::new_with_events`] to wire a shared dispatcher (e.g.
    /// one owned by [`TaskHandler`](crate::worker::TaskHandler)) so the
    /// `MetricsCollector` can observe workflow-start metrics.
    pub fn new(api: ApiClient) -> Self {
        Self {
            api,
            events: EventDispatcher::default(),
        }
    }

    /// Create a new workflow client wired to an existing [`EventDispatcher`].
    pub fn new_with_events(api: ApiClient, events: EventDispatcher) -> Self {
        Self { api, events }
    }

    /// Start a workflow asynchronously
    pub async fn start_workflow(&self, request: &StartWorkflowRequest) -> Result<String> {
        debug!(
            workflow_name = %request.name,
            "Starting workflow"
        );

        // Compute input byte size up-front so it is available for both the
        // success-path gauge and for the failure-path tracing. Uses the same
        // JSON serialization that the transport will perform, so the reported
        // bytes match what actually leaves this process.
        let input_size_bytes = serde_json::to_vec(&request.input)
            .map(|v| v.len())
            .unwrap_or(0);

        match self
            .api
            .post_text::<StartWorkflowRequest>("/workflow", request)
            .await
        {
            Ok(workflow_id) => {
                info!(
                    workflow_name = %request.name,
                    workflow_id = %workflow_id,
                    "Workflow started"
                );

                self.events.publish_workflow_started(&WorkflowStarted::new(
                    &request.name,
                    request.version,
                    input_size_bytes,
                ));

                Ok(workflow_id)
            }
            Err(e) => {
                let exception = exception_label(&e);
                self.events
                    .publish_workflow_start_failure(&WorkflowStartFailure::new(
                        &request.name,
                        exception,
                    ));
                Err(e)
            }
        }
    }

    /// Execute a workflow synchronously and wait for completion
    pub async fn execute_workflow(
        &self,
        request: &StartWorkflowRequest,
        wait_for: Duration,
    ) -> Result<Workflow> {
        let wait_secs = wait_for.as_secs().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();

        let path = format!(
            "/workflow/execute/{}/{}?waitForSeconds={}",
            request.name,
            request.version.unwrap_or(1),
            wait_secs
        );

        debug!(
            workflow_name = %request.name,
            wait_secs = %wait_secs,
            "Executing workflow synchronously"
        );

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExecuteRequest<'a> {
            #[serde(flatten)]
            request: &'a StartWorkflowRequest,
            request_id: String,
        }

        let exec_request = ExecuteRequest {
            request,
            request_id,
        };

        let workflow: Workflow = self.api.post(&path, &exec_request).await?;

        info!(
            workflow_name = %request.name,
            workflow_id = %workflow.workflow_id,
            status = ?workflow.status,
            "Workflow executed"
        );

        Ok(workflow)
    }

    /// Get workflow by ID
    pub async fn get_workflow(&self, workflow_id: &str, include_tasks: bool) -> Result<Workflow> {
        let path = format!("/workflow/{}?includeTasks={}", workflow_id, include_tasks);
        self.api.get(&path).await
    }

    /// Get workflow status
    pub async fn get_workflow_status(
        &self,
        workflow_id: &str,
        include_output: bool,
        include_variables: bool,
    ) -> Result<Workflow> {
        let path = format!(
            "/workflow/{}/status?includeOutput={}&includeVariables={}",
            workflow_id, include_output, include_variables
        );
        self.api.get(&path).await
    }

    /// Terminate a running workflow
    pub async fn terminate_workflow(
        &self,
        workflow_id: &str,
        reason: Option<&str>,
        trigger_failure_workflow: bool,
    ) -> Result<()> {
        let mut path = format!(
            "/workflow/{}?triggerFailureWorkflow={}",
            workflow_id, trigger_failure_workflow
        );

        if let Some(r) = reason {
            path.push_str(&format!("&reason={}", urlencoding::encode(r)));
        }

        self.api.delete_no_content(&path).await
    }

    /// Pause a running workflow
    pub async fn pause_workflow(&self, workflow_id: &str) -> Result<()> {
        let path = format!("/workflow/{}/pause", workflow_id);
        let _: serde_json::Value = self.api.put(&path, &serde_json::Value::Null).await?;
        Ok(())
    }

    /// Resume a paused workflow
    pub async fn resume_workflow(&self, workflow_id: &str) -> Result<()> {
        let path = format!("/workflow/{}/resume", workflow_id);
        let _: serde_json::Value = self.api.put(&path, &serde_json::Value::Null).await?;
        Ok(())
    }

    /// Retry a failed workflow
    pub async fn retry_workflow(
        &self,
        workflow_id: &str,
        resume_subworkflow_tasks: bool,
    ) -> Result<()> {
        let path = format!(
            "/workflow/{}/retry?resumeSubworkflowTasks={}",
            workflow_id, resume_subworkflow_tasks
        );
        let _: serde_json::Value = self.api.post(&path, &serde_json::Value::Null).await?;
        Ok(())
    }

    /// Restart a workflow from the beginning
    pub async fn restart_workflow(&self, workflow_id: &str, use_latest_def: bool) -> Result<()> {
        let path = format!(
            "/workflow/{}/restart?useLatestDefinitions={}",
            workflow_id, use_latest_def
        );
        let _: serde_json::Value = self.api.post(&path, &serde_json::Value::Null).await?;
        Ok(())
    }

    /// Rerun a workflow from a specific task
    pub async fn rerun_workflow(
        &self,
        workflow_id: &str,
        rerun_from_task_id: &str,
        task_input: Option<HashMap<String, serde_json::Value>>,
        workflow_input: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        let path = format!("/workflow/{}/rerun", workflow_id);

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RerunRequest {
            re_run_from_task_id: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            task_input: Option<HashMap<String, serde_json::Value>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            workflow_input: Option<HashMap<String, serde_json::Value>>,
        }

        let request = RerunRequest {
            re_run_from_task_id: rerun_from_task_id.to_string(),
            task_input,
            workflow_input,
        };

        self.api.post(&path, &request).await
    }

    /// Update workflow variables
    pub async fn update_variables(
        &self,
        workflow_id: &str,
        variables: HashMap<String, serde_json::Value>,
    ) -> Result<Workflow> {
        let path = format!("/workflow/{}/variables", workflow_id);
        self.api.post(&path, &variables).await
    }

    /// Skip a task in a running workflow
    pub async fn skip_task(&self, workflow_id: &str, task_reference_name: &str) -> Result<()> {
        let path = format!("/workflow/{}/skiptask/{}", workflow_id, task_reference_name);

        #[derive(serde::Serialize)]
        struct SkipRequest {}

        let _: serde_json::Value = self.api.put(&path, &SkipRequest {}).await?;
        Ok(())
    }

    /// Search for workflows
    pub async fn search_workflows(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<SearchResult<Workflow>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_string()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_string()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api.get_with_params("/workflow/search", &params).await
    }

    /// Search for workflows V2 (returns full workflow objects)
    pub async fn search_workflows_v2(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<SearchResult<Workflow>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_string()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_string()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api
            .get_with_params("/workflow/search-v2", &params)
            .await
    }

    /// Skip a task from workflow (alias for skip_task)
    pub async fn skip_task_from_workflow(
        &self,
        workflow_id: &str,
        task_reference_name: &str,
    ) -> Result<()> {
        self.skip_task(workflow_id, task_reference_name).await
    }

    /// Bulk pause workflows
    pub async fn pause_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        let path = "/workflow/bulk/pause";
        self.api.put(path, workflow_ids).await
    }

    /// Bulk resume workflows
    pub async fn resume_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        let path = "/workflow/bulk/resume";
        self.api.put(path, workflow_ids).await
    }

    /// Bulk restart workflows
    pub async fn restart_workflows(
        &self,
        workflow_ids: &[String],
        use_latest_def: bool,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let path = format!(
            "/workflow/bulk/restart?useLatestDefinitions={}",
            use_latest_def
        );
        self.api.post(&path, workflow_ids).await
    }

    /// Bulk retry workflows
    pub async fn retry_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        let path = "/workflow/bulk/retry";
        self.api.post(path, workflow_ids).await
    }

    /// Bulk terminate workflows
    pub async fn terminate_workflows(
        &self,
        workflow_ids: &[String],
        reason: Option<&str>,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut path = "/workflow/bulk/terminate".to_string();
        if let Some(r) = reason {
            path.push_str(&format!("?reason={}", urlencoding::encode(r)));
        }
        self.api.post(&path, workflow_ids).await
    }

    /// Get running workflows by name
    pub async fn get_running_workflows(
        &self,
        workflow_name: &str,
        version: Option<i32>,
        start_time: Option<i64>,
        end_time: Option<i64>,
    ) -> Result<Vec<String>> {
        let mut path = format!("/workflow/running/{}", workflow_name);

        let mut params = vec![];
        let version_str;
        let start_str;
        let end_str;

        if let Some(v) = version {
            version_str = v.to_string();
            params.push(("version", version_str.as_str()));
        }
        if let Some(s) = start_time {
            start_str = s.to_string();
            params.push(("startTime", start_str.as_str()));
        }
        if let Some(e) = end_time {
            end_str = e.to_string();
            params.push(("endTime", end_str.as_str()));
        }

        if !params.is_empty() {
            path.push('?');
            path.push_str(
                &params
                    .into_iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }

        self.api.get(&path).await
    }

    /// Delete a workflow execution
    pub async fn delete_workflow(&self, workflow_id: &str, archive: bool) -> Result<()> {
        let path = format!("/workflow/{}?archiveWorkflow={}", workflow_id, archive);
        self.api.delete_no_content(&path).await
    }

    /// Test a workflow (dry run)
    pub async fn test_workflow(&self, request: &TestWorkflowRequest) -> Result<Workflow> {
        self.api.post("/workflow/test", request).await
    }

    /// Remove/delete a workflow
    pub async fn remove_workflow(&self, workflow_id: &str) -> Result<()> {
        self.delete_workflow(workflow_id, false).await
    }

    /// Get workflows by correlation IDs
    pub async fn get_by_correlation_ids(
        &self,
        workflow_name: &str,
        correlation_ids: &[String],
        include_completed: bool,
        include_tasks: bool,
    ) -> Result<HashMap<String, Vec<Workflow>>> {
        let path = format!(
            "/workflow/{}/correlated?includeClosed={}&includeTasks={}",
            workflow_name, include_completed, include_tasks
        );
        self.api.post(&path, correlation_ids).await
    }

    /// Get workflows by correlation IDs in batch
    pub async fn get_by_correlation_ids_in_batch(
        &self,
        batch_request: &CorrelationIdsSearchRequest,
        include_completed: bool,
        include_tasks: bool,
    ) -> Result<HashMap<String, Vec<Workflow>>> {
        let path = format!(
            "/workflow/correlated/batch?includeClosed={}&includeTasks={}",
            include_completed, include_tasks
        );
        self.api.post(&path, batch_request).await
    }

    /// Update workflow state
    pub async fn update_state(
        &self,
        workflow_id: &str,
        update_request: &WorkflowStateUpdate,
        wait_until_task_ref_names: Option<&[String]>,
        wait_for_seconds: Option<i32>,
    ) -> Result<WorkflowRun> {
        let mut path = format!("/workflow/{}/state", workflow_id);
        let mut params: Vec<String> = vec![];

        if let Some(refs) = wait_until_task_ref_names {
            for r in refs {
                params.push(format!("waitUntilTaskRefNames={}", urlencoding::encode(r)));
            }
        }
        if let Some(secs) = wait_for_seconds {
            params.push(format!("waitForSeconds={}", secs));
        }

        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        self.api.post(&path, update_request).await
    }

    /// Execute workflow with return strategy
    pub async fn execute_workflow_with_return_strategy(
        &self,
        request: &StartWorkflowRequest,
        request_id: Option<&str>,
        wait_until_task_ref: Option<&str>,
        wait_for_seconds: i32,
        consistency: Option<&str>,
        return_strategy: Option<&str>,
    ) -> Result<SignalResponse> {
        let mut path = format!(
            "/workflow/execute/{}/{}",
            request.name,
            request.version.unwrap_or(1)
        );

        let mut params: Vec<String> = vec![];
        params.push(format!("waitForSeconds={}", wait_for_seconds));

        if let Some(rid) = request_id {
            params.push(format!("requestId={}", urlencoding::encode(rid)));
        }
        if let Some(task_ref) = wait_until_task_ref {
            params.push(format!(
                "waitUntilTaskRef={}",
                urlencoding::encode(task_ref)
            ));
        }
        if let Some(c) = consistency {
            params.push(format!("consistency={}", c));
        }
        if let Some(rs) = return_strategy {
            params.push(format!("returnStrategy={}", rs));
        }

        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        self.api.post(&path, request).await
    }
}

/// Correlation IDs search request
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationIdsSearchRequest {
    /// Correlation IDs to search for
    #[serde(default)]
    pub correlation_ids: Vec<String>,

    /// Workflow names to search in
    #[serde(default)]
    pub workflow_names: Vec<String>,
}

impl CorrelationIdsSearchRequest {
    /// Create a new request
    pub fn new() -> Self {
        Self::default()
    }

    /// Add correlation IDs
    pub fn with_correlation_ids(mut self, ids: Vec<String>) -> Self {
        self.correlation_ids = ids;
        self
    }

    /// Add workflow names
    pub fn with_workflow_names(mut self, names: Vec<String>) -> Self {
        self.workflow_names = names;
        self
    }
}

/// Workflow state update request
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStateUpdate {
    /// Task reference name to update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reference_name: Option<String>,

    /// Variables to update
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Task output
    #[serde(default)]
    pub task_result: Option<crate::models::TaskResult>,
}

/// Workflow run result
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    /// Workflow ID
    #[serde(default)]
    pub workflow_id: String,

    /// Workflow status
    #[serde(default)]
    pub status: crate::models::WorkflowStatus,

    /// Output
    #[serde(default)]
    pub output: HashMap<String, serde_json::Value>,

    /// Variables
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Tasks
    #[serde(default)]
    pub tasks: Vec<crate::models::Task>,
}

/// Signal response
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalResponse {
    /// Workflow ID
    #[serde(default)]
    pub workflow_id: String,

    /// Status
    #[serde(default)]
    pub status: crate::models::WorkflowStatus,

    /// Output
    #[serde(default)]
    pub output: HashMap<String, serde_json::Value>,
}

/// Search result with pagination
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult<T> {
    /// Total number of hits
    pub total_hits: i64,

    /// Results in this page
    pub results: Vec<T>,
}

/// Request for testing a workflow
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestWorkflowRequest {
    /// Workflow name
    pub name: String,

    /// Workflow version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    /// Workflow definition (optional, uses registered if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_def: Option<WorkflowDef>,

    /// Task reference to mock output mapping
    #[serde(default)]
    pub task_ref_to_mock_output: HashMap<String, HashMap<String, serde_json::Value>>,

    /// Workflow input
    #[serde(default)]
    pub workflow_input: HashMap<String, serde_json::Value>,
}

impl TestWorkflowRequest {
    /// Create a new test workflow request
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            workflow_def: None,
            task_ref_to_mock_output: HashMap::new(),
            workflow_input: HashMap::new(),
        }
    }

    /// Set version
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set workflow definition
    pub fn with_workflow_def(mut self, def: WorkflowDef) -> Self {
        self.workflow_def = Some(def);
        self
    }

    /// Add mock output for a task
    pub fn with_mock_output(
        mut self,
        task_ref: impl Into<String>,
        output: HashMap<String, serde_json::Value>,
    ) -> Self {
        self.task_ref_to_mock_output.insert(task_ref.into(), output);
        self
    }

    /// Set workflow input
    pub fn with_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.workflow_input = input;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_workflow_request() {
        let request = StartWorkflowRequest::new("test_workflow")
            .with_version(1)
            .with_input_value("key", "value");

        assert_eq!(request.name, "test_workflow");
        assert_eq!(request.version, Some(1));
    }

    #[test]
    fn test_test_workflow_request() {
        let mut mock_output = HashMap::new();
        mock_output.insert("result".to_string(), serde_json::json!("success"));

        let request = TestWorkflowRequest::new("test_workflow")
            .with_version(1)
            .with_mock_output("task_ref", mock_output);

        assert_eq!(request.name, "test_workflow");
        assert!(request.task_ref_to_mock_output.contains_key("task_ref"));
    }
}
