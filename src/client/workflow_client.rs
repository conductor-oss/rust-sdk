// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;
use tracing::{debug, info};

use crate::error::Result;
use crate::events::{exception_label, EventDispatcher, WorkflowStartFailure, WorkflowStarted};
use crate::http::{ApiClient, ApiPath};
use crate::models::{StartWorkflowRequest, TaskResultStatus, Workflow, WorkflowDef};

/// Client for workflow operations.
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
    #[must_use]
    pub fn new(api: ApiClient) -> Self {
        Self {
            api,
            events: EventDispatcher::default(),
        }
    }

    /// Create a new workflow client wired to an existing [`EventDispatcher`].
    #[must_use]
    pub fn new_with_events(api: ApiClient, events: EventDispatcher) -> Self {
        Self { api, events }
    }

    /// Start a workflow asynchronously.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn start_workflow(&self, request: &StartWorkflowRequest) -> Result<String> {
        debug!(
            workflow_name = %request.name,
            "Starting workflow"
        );

        // Compute input byte size up-front so it is available for both the
        // success-path gauge and for the failure-path tracing. Uses the same
        // JSON serialization that the transport will perform, so the reported
        // bytes match what actually leaves this process.
        let input_size_bytes = serde_json::to_vec(&request.input).map_or(0, |v| v.len());

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

    /// Execute a workflow synchronously and wait for completion.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn execute_workflow(
        &self,
        request: &StartWorkflowRequest,
        wait_for: Duration,
    ) -> Result<Workflow> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ExecuteRequest<'a> {
            #[serde(flatten)]
            request: &'a StartWorkflowRequest,
            request_id: String,
        }

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

        let exec_request = ExecuteRequest {
            request,
            request_id,
        };

        let workflow: Workflow = self
            .api
            .post(
                ApiPath::templated(&path, "/workflow/execute/{name}/{version}"),
                &exec_request,
            )
            .await?;

        info!(
            workflow_name = %request.name,
            workflow_id = %workflow.workflow_id,
            status = ?workflow.status,
            "Workflow executed"
        );

        Ok(workflow)
    }

    /// Get workflow by ID.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_workflow(&self, workflow_id: &str, include_tasks: bool) -> Result<Workflow> {
        let path = format!("/workflow/{workflow_id}?includeTasks={include_tasks}");
        self.api
            .get(ApiPath::templated(&path, "/workflow/{workflowId}"))
            .await
    }

    /// Get workflow status.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_workflow_status(
        &self,
        workflow_id: &str,
        include_output: bool,
        include_variables: bool,
    ) -> Result<Workflow> {
        let path = format!(
            "/workflow/{workflow_id}/status?includeOutput={include_output}&includeVariables={include_variables}"
        );
        self.api
            .get(ApiPath::templated(&path, "/workflow/{workflowId}/status"))
            .await
    }

    /// Push a message into a running workflow's Workflow Message Queue (WMQ), waking up any
    /// [`crate::models::WorkflowTask::pull_workflow_messages`] task currently waiting on it. `message` may be
    /// any JSON-serializable value. Returns the server-generated message ID.
    ///
    /// Requires `conductor.workflow-message-queue.enabled=true` on the target server -- the
    /// endpoint doesn't exist at all otherwise (a 404 from the server, not a client-side check).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport
    /// level. Returns [`crate::error::ConductorError::Server`] if the workflow isn't in a
    /// `RUNNING` state (409), the queue is at capacity (429, default 1000 messages -- see
    /// `conductor.workflow-message-queue.maxQueueSize`), or another non-2xx status. Returns
    /// [`crate::error::ConductorError::Api`] if `workflow_id` doesn't exist (404).
    pub async fn send_message(
        &self,
        workflow_id: &str,
        message: &serde_json::Value,
    ) -> Result<String> {
        let path = format!("/workflow/{workflow_id}/messages");
        self.api
            .post_text(
                ApiPath::templated(&path, "/workflow/{workflowId}/messages"),
                message,
            )
            .await
    }

    /// Terminate a running workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn terminate_workflow(
        &self,
        workflow_id: &str,
        reason: Option<&str>,
        trigger_failure_workflow: bool,
    ) -> Result<()> {
        let mut path =
            format!("/workflow/{workflow_id}?triggerFailureWorkflow={trigger_failure_workflow}");

        if let Some(r) = reason {
            let _ = write!(path, "&reason={}", urlencoding::encode(r));
        }

        self.api
            .delete_no_content(ApiPath::templated(&path, "/workflow/{workflowId}"))
            .await
    }

    /// Pause a running workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn pause_workflow(&self, workflow_id: &str) -> Result<()> {
        let path = format!("/workflow/{workflow_id}/pause");
        let _: serde_json::Value = self
            .api
            .put(
                ApiPath::templated(&path, "/workflow/{workflowId}/pause"),
                &serde_json::Value::Null,
            )
            .await?;
        Ok(())
    }

    /// Resume a paused workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn resume_workflow(&self, workflow_id: &str) -> Result<()> {
        let path = format!("/workflow/{workflow_id}/resume");
        let _: serde_json::Value = self
            .api
            .put(
                ApiPath::templated(&path, "/workflow/{workflowId}/resume"),
                &serde_json::Value::Null,
            )
            .await?;
        Ok(())
    }

    /// Retry a failed workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn retry_workflow(
        &self,
        workflow_id: &str,
        resume_subworkflow_tasks: bool,
    ) -> Result<()> {
        let path = format!(
            "/workflow/{workflow_id}/retry?resumeSubworkflowTasks={resume_subworkflow_tasks}"
        );
        let _: serde_json::Value = self
            .api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowId}/retry"),
                &serde_json::Value::Null,
            )
            .await?;
        Ok(())
    }

    /// Restart a workflow from the beginning.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn restart_workflow(&self, workflow_id: &str, use_latest_def: bool) -> Result<()> {
        let path = format!("/workflow/{workflow_id}/restart?useLatestDefinitions={use_latest_def}");
        let _: serde_json::Value = self
            .api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowId}/restart"),
                &serde_json::Value::Null,
            )
            .await?;
        Ok(())
    }

    /// Rerun a workflow from a specific task.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn rerun_workflow(
        &self,
        workflow_id: &str,
        rerun_from_task_id: &str,
        task_input: Option<HashMap<String, serde_json::Value>>,
        workflow_input: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RerunRequest {
            re_run_from_task_id: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            task_input: Option<HashMap<String, serde_json::Value>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            workflow_input: Option<HashMap<String, serde_json::Value>>,
        }

        let path = format!("/workflow/{workflow_id}/rerun");

        let request = RerunRequest {
            re_run_from_task_id: rerun_from_task_id.to_owned(),
            task_input,
            workflow_input,
        };

        self.api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowId}/rerun"),
                &request,
            )
            .await
    }

    /// Update workflow variables.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn update_variables(
        &self,
        workflow_id: &str,
        variables: HashMap<String, serde_json::Value>,
    ) -> Result<Workflow> {
        let path = format!("/workflow/{workflow_id}/variables");
        self.api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowId}/variables"),
                &variables,
            )
            .await
    }

    /// Skip a task in a running workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn skip_task(&self, workflow_id: &str, task_reference_name: &str) -> Result<()> {
        // Kept as `{}` rather than a unit struct: serde serializes a unit struct as JSON `null`,
        // but the server expects an empty JSON object body here.
        #[expect(clippy::empty_structs_with_brackets)]
        #[derive(serde::Serialize)]
        struct SkipRequest {}

        let path = format!("/workflow/{workflow_id}/skiptask/{task_reference_name}");

        let _: serde_json::Value = self
            .api
            .put(
                ApiPath::templated(&path, "/workflow/{workflowId}/skiptask/{taskRefName}"),
                &SkipRequest {},
            )
            .await?;
        Ok(())
    }

    /// Search for workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn search_workflows(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<SearchResult<Workflow>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_owned()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_owned()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api.get_with_params("/workflow/search", &params).await
    }

    /// Search for workflows V2 (returns full workflow objects).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn search_workflows_v2(
        &self,
        query: Option<&str>,
        free_text: Option<&str>,
        start: i32,
        size: i32,
    ) -> Result<SearchResult<Workflow>> {
        let mut params = vec![("start", start.to_string()), ("size", size.to_string())];

        if let Some(q) = query {
            params.push(("query", q.to_owned()));
        }
        if let Some(ft) = free_text {
            params.push(("freeText", ft.to_owned()));
        }

        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        self.api
            .get_with_params("/workflow/search-v2", &params)
            .await
    }

    /// Skip a task from workflow (alias for `skip_task`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn skip_task_from_workflow(
        &self,
        workflow_id: &str,
        task_reference_name: &str,
    ) -> Result<()> {
        self.skip_task(workflow_id, task_reference_name).await
    }

    /// Bulk pause workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn pause_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        self.api.put("/workflow/bulk/pause", workflow_ids).await
    }

    /// Bulk resume workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn resume_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        self.api.put("/workflow/bulk/resume", workflow_ids).await
    }

    /// Bulk restart workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn restart_workflows(
        &self,
        workflow_ids: &[String],
        use_latest_def: bool,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let path = format!("/workflow/bulk/restart?useLatestDefinitions={use_latest_def}");
        self.api
            .post(
                ApiPath::templated(&path, "/workflow/bulk/restart"),
                workflow_ids,
            )
            .await
    }

    /// Bulk retry workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn retry_workflows(
        &self,
        workflow_ids: &[String],
    ) -> Result<HashMap<String, serde_json::Value>> {
        self.api.post("/workflow/bulk/retry", workflow_ids).await
    }

    /// Bulk terminate workflows.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn terminate_workflows(
        &self,
        workflow_ids: &[String],
        reason: Option<&str>,
    ) -> Result<HashMap<String, serde_json::Value>> {
        let mut path = "/workflow/bulk/terminate".to_owned();
        if let Some(r) = reason {
            let _ = write!(path, "?reason={}", urlencoding::encode(r));
        }
        self.api
            .post(
                ApiPath::templated(&path, "/workflow/bulk/terminate"),
                workflow_ids,
            )
            .await
    }

    /// Get running workflows by name.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_running_workflows(
        &self,
        workflow_name: &str,
        version: Option<i32>,
        start_time: Option<i64>,
        end_time: Option<i64>,
    ) -> Result<Vec<String>> {
        let mut path = format!("/workflow/running/{workflow_name}");

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
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }

        self.api
            .get(ApiPath::templated(
                &path,
                "/workflow/running/{workflowName}",
            ))
            .await
    }

    /// Delete a workflow execution.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn delete_workflow(&self, workflow_id: &str, archive: bool) -> Result<()> {
        let path = format!("/workflow/{workflow_id}?archiveWorkflow={archive}");
        self.api
            .delete_no_content(ApiPath::templated(&path, "/workflow/{workflowId}"))
            .await
    }

    /// Test a workflow (dry run).
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn test_workflow(&self, request: &TestWorkflowRequest) -> Result<Workflow> {
        self.api.post("/workflow/test", request).await
    }

    /// Remove/delete a workflow.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn remove_workflow(&self, workflow_id: &str) -> Result<()> {
        self.delete_workflow(workflow_id, false).await
    }

    /// Get workflows by correlation IDs.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_by_correlation_ids(
        &self,
        workflow_name: &str,
        correlation_ids: &[String],
        include_completed: bool,
        include_tasks: bool,
    ) -> Result<HashMap<String, Vec<Workflow>>> {
        let path = format!(
            "/workflow/{workflow_name}/correlated?includeClosed={include_completed}&includeTasks={include_tasks}"
        );
        self.api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowName}/correlated"),
                correlation_ids,
            )
            .await
    }

    /// Get workflows by correlation IDs in batch.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_by_correlation_ids_in_batch(
        &self,
        batch_request: &CorrelationIdsSearchRequest,
        include_completed: bool,
        include_tasks: bool,
    ) -> Result<HashMap<String, Vec<Workflow>>> {
        let path = format!(
            "/workflow/correlated/batch?includeClosed={include_completed}&includeTasks={include_tasks}"
        );
        self.api
            .post(
                ApiPath::templated(&path, "/workflow/correlated/batch"),
                batch_request,
            )
            .await
    }

    /// Update workflow state.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn update_state(
        &self,
        workflow_id: &str,
        update_request: &WorkflowStateUpdate,
        wait_until_task_ref_names: Option<&[String]>,
        wait_for_seconds: Option<i32>,
    ) -> Result<WorkflowRun> {
        let mut path = format!("/workflow/{workflow_id}/state");
        let mut params: Vec<String> = vec![];

        if let Some(refs) = wait_until_task_ref_names {
            for r in refs {
                params.push(format!("waitUntilTaskRefNames={}", urlencoding::encode(r)));
            }
        }
        if let Some(secs) = wait_for_seconds {
            params.push(format!("waitForSeconds={secs}"));
        }

        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        self.api
            .post(
                ApiPath::templated(&path, "/workflow/{workflowId}/state"),
                update_request,
            )
            .await
    }

    /// Execute workflow with return strategy.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
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
        params.push(format!("waitForSeconds={wait_for_seconds}"));

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
            params.push(format!("consistency={c}"));
        }
        if let Some(rs) = return_strategy {
            params.push(format!("returnStrategy={rs}"));
        }

        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        self.api
            .post(
                ApiPath::templated(&path, "/workflow/execute/{name}/{version}"),
                request,
            )
            .await
    }
}

/// Correlation IDs search request.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationIdsSearchRequest {
    /// Correlation IDs to search for.
    #[serde(default)]
    pub correlation_ids: Vec<String>,

    /// Workflow names to search in.
    #[serde(default)]
    pub workflow_names: Vec<String>,
}

impl CorrelationIdsSearchRequest {
    /// Create a new request.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add correlation IDs.
    #[must_use]
    pub fn with_correlation_ids(mut self, ids: Vec<String>) -> Self {
        self.correlation_ids = ids;
        self
    }

    /// Add workflow names.
    #[must_use]
    pub fn with_workflow_names(mut self, names: Vec<String>) -> Self {
        self.workflow_names = names;
        self
    }
}

/// Workflow state update request.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStateUpdate {
    /// Task reference name to update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reference_name: Option<String>,

    /// Variables to update.
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Task output.
    #[serde(default)]
    pub task_result: Option<crate::models::TaskResult>,
}

/// Workflow run result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    /// Workflow ID.
    #[serde(default)]
    pub workflow_id: String,

    /// Workflow status.
    #[serde(default)]
    pub status: crate::models::WorkflowStatus,

    /// Output.
    #[serde(default)]
    pub output: HashMap<String, serde_json::Value>,

    /// Variables.
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Tasks.
    #[serde(default)]
    pub tasks: Vec<crate::models::Task>,
}

/// Signal response.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalResponse {
    /// Workflow ID.
    #[serde(default)]
    pub workflow_id: String,

    /// Status.
    #[serde(default)]
    pub status: crate::models::WorkflowStatus,

    /// Output.
    #[serde(default)]
    pub output: HashMap<String, serde_json::Value>,
}

/// Search result with pagination.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult<T> {
    /// Total number of hits.
    pub total_hits: i64,

    /// Results in this page.
    pub results: Vec<T>,
}

/// One simulated task attempt for [`TestWorkflowRequest::with_mock_outputs`], matching the
/// server's `WorkflowTestRequest.TaskMock` exactly (`status`/`output`/`executionTime`/
/// `queueWaitTime`). Multiple entries for the same task reference simulate a retry sequence:
/// the first entry is attempt 1, the second is attempt 2 if the workflow retries, and so on.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMock {
    /// Simulated task status for this attempt.
    pub status: TaskResultStatus,

    /// Simulated task output for this attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<HashMap<String, serde_json::Value>>,

    /// Simulated execution time in milliseconds -- useful for testing timeout handling.
    #[serde(default)]
    pub execution_time: i64,

    /// Simulated queue wait time in milliseconds.
    #[serde(default)]
    pub queue_wait_time: i64,
}

impl TaskMock {
    /// A mock `COMPLETED` attempt with the given output -- the common case.
    #[must_use]
    pub fn completed(output: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status: TaskResultStatus::Completed,
            output: Some(output),
            execution_time: 0,
            queue_wait_time: 0,
        }
    }

    /// A mock attempt with an explicit status (e.g. to simulate a retryable failure before a
    /// later `completed()` attempt succeeds).
    #[must_use]
    pub fn new(status: TaskResultStatus, output: HashMap<String, serde_json::Value>) -> Self {
        Self {
            status,
            output: Some(output),
            execution_time: 0,
            queue_wait_time: 0,
        }
    }

    /// Set the simulated execution time, for testing timeout-handling logic.
    #[must_use]
    pub fn with_execution_time(mut self, millis: i64) -> Self {
        self.execution_time = millis;
        self
    }

    /// Set the simulated queue wait time.
    #[must_use]
    pub fn with_queue_wait_time(mut self, millis: i64) -> Self {
        self.queue_wait_time = millis;
        self
    }
}

/// Request for testing a workflow. Extends [`StartWorkflowRequest`]'s field set with test-only
/// fields (`task_ref_to_mock_output`/`sub_workflow_test_request`), matching the server's
/// `WorkflowTestRequest extends StartWorkflowRequest` exactly -- this crate doesn't have struct
/// inheritance, so the shared fields are simply duplicated here rather than composed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestWorkflowRequest {
    /// Workflow name.
    pub name: String,

    /// Workflow version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    /// Workflow input.
    #[serde(default)]
    pub input: HashMap<String, serde_json::Value>,

    /// Correlation ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Task to domain mapping.
    #[serde(default)]
    pub task_to_domain: HashMap<String, String>,

    /// Workflow definition (optional, uses registered if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_def: Option<WorkflowDef>,

    /// External input payload storage path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_input_payload_storage_path: Option<String>,

    /// Priority.
    #[serde(default)]
    pub priority: i32,

    /// Task reference to mocked-attempt-sequence mapping. Each entry is a [`TaskMock`] list, not
    /// a single output -- see [`TestWorkflowRequest::with_mock_output`]/
    /// [`TestWorkflowRequest::with_mock_outputs`].
    #[serde(default)]
    pub task_ref_to_mock_output: HashMap<String, Vec<TaskMock>>,

    /// Per-sub-workflow-task-reference test request, for mocking task outputs inside a
    /// sub-workflow the same way as the top-level workflow.
    #[serde(default)]
    pub sub_workflow_test_request: HashMap<String, TestWorkflowRequest>,
}

impl TestWorkflowRequest {
    /// Create a new test workflow request.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            input: HashMap::new(),
            correlation_id: None,
            task_to_domain: HashMap::new(),
            workflow_def: None,
            external_input_payload_storage_path: None,
            priority: 0,
            task_ref_to_mock_output: HashMap::new(),
            sub_workflow_test_request: HashMap::new(),
        }
    }

    /// Set version.
    #[must_use]
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set workflow definition.
    #[must_use]
    pub fn with_workflow_def(mut self, def: WorkflowDef) -> Self {
        self.workflow_def = Some(def);
        self
    }

    /// Set workflow input.
    #[must_use]
    pub fn with_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.input = input;
        self
    }

    /// Set correlation ID.
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Set task-to-domain routing.
    #[must_use]
    pub fn with_task_to_domain(mut self, task_to_domain: HashMap<String, String>) -> Self {
        self.task_to_domain = task_to_domain;
        self
    }

    /// Set priority.
    #[must_use]
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Append a single `COMPLETED` mock attempt with `output` for `task_ref`. Calling this
    /// multiple times for the same `task_ref` builds a retry sequence -- the attempt from the
    /// first call runs first. For non-`COMPLETED` attempts (simulating a failure before a
    /// retry succeeds) or execution/queue-time simulation, use
    /// [`TestWorkflowRequest::with_mock_outputs`] with [`TaskMock`] directly.
    #[must_use]
    pub fn with_mock_output(
        mut self,
        task_ref: impl Into<String>,
        output: HashMap<String, serde_json::Value>,
    ) -> Self {
        self.task_ref_to_mock_output
            .entry(task_ref.into())
            .or_default()
            .push(TaskMock::completed(output));
        self
    }

    /// Set the full mocked-attempt sequence for `task_ref`, replacing any previous mocks for it.
    #[must_use]
    pub fn with_mock_outputs(mut self, task_ref: impl Into<String>, mocks: Vec<TaskMock>) -> Self {
        self.task_ref_to_mock_output.insert(task_ref.into(), mocks);
        self
    }

    /// Add a mock test request for the sub-workflow spawned at `task_ref`.
    #[must_use]
    pub fn with_sub_workflow_test_request(
        mut self,
        task_ref: impl Into<String>,
        request: TestWorkflowRequest,
    ) -> Self {
        self.sub_workflow_test_request
            .insert(task_ref.into(), request);
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
        mock_output.insert("result".to_owned(), serde_json::json!("success"));

        let request = TestWorkflowRequest::new("test_workflow")
            .with_version(1)
            .with_mock_output("task_ref", mock_output);

        assert_eq!(request.name, "test_workflow");
        assert!(request.task_ref_to_mock_output.contains_key("task_ref"));
        assert_eq!(request.task_ref_to_mock_output["task_ref"].len(), 1);
        assert_eq!(
            request.task_ref_to_mock_output["task_ref"][0].status,
            TaskResultStatus::Completed
        );
    }

    #[test]
    fn test_mock_output_appends_a_retry_sequence() {
        let mut first_attempt = HashMap::new();
        first_attempt.insert("attempt".to_owned(), serde_json::json!(1));
        let mut second_attempt = HashMap::new();
        second_attempt.insert("attempt".to_owned(), serde_json::json!(2));

        let request = TestWorkflowRequest::new("test_workflow")
            .with_mock_outputs(
                "flaky_ref",
                vec![TaskMock::new(TaskResultStatus::Failed, first_attempt)],
            )
            .with_mock_output("flaky_ref", second_attempt);

        let mocks = &request.task_ref_to_mock_output["flaky_ref"];
        assert_eq!(mocks.len(), 2);
        assert_eq!(mocks[0].status, TaskResultStatus::Failed);
        assert_eq!(mocks[1].status, TaskResultStatus::Completed);
    }

    #[test]
    fn test_test_workflow_request_serializes_with_correct_wire_keys() {
        let mut output = HashMap::new();
        output.insert("ok".to_owned(), serde_json::json!(true));

        let request = TestWorkflowRequest::new("test_workflow")
            .with_input(HashMap::from([(
                "orderId".to_owned(),
                serde_json::json!("ORD-1"),
            )]))
            .with_mock_output("task_ref", output);

        let json = serde_json::to_value(&request).unwrap();
        // `input`, not `workflowInput`; `taskRefToMockOutput` values are arrays, not objects.
        assert_eq!(json["input"]["orderId"], serde_json::json!("ORD-1"));
        assert!(json["taskRefToMockOutput"]["task_ref"].is_array());
        assert_eq!(
            json["taskRefToMockOutput"]["task_ref"][0]["status"],
            serde_json::json!("COMPLETED")
        );
    }
}
