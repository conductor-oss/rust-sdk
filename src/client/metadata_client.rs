// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use tracing::{debug, info};

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::{TaskDef, WorkflowDef};

/// Client for metadata operations (workflow and task definitions)
#[derive(Clone)]
pub struct MetadataClient {
    api: ApiClient,
}

impl MetadataClient {
    /// Create a new metadata client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    // ==================== Workflow Definitions ====================

    /// Register a new workflow definition
    pub async fn register_workflow_def(&self, workflow: &WorkflowDef) -> Result<()> {
        debug!(
            workflow_name = %workflow.name,
            version = workflow.version,
            "Registering workflow definition"
        );

        let _: serde_json::Value = self.api.post("/metadata/workflow", workflow).await?;

        info!(
            workflow_name = %workflow.name,
            version = workflow.version,
            "Workflow definition registered"
        );

        Ok(())
    }

    /// Update an existing workflow definition
    pub async fn update_workflow_def(&self, workflow: &WorkflowDef) -> Result<()> {
        debug!(
            workflow_name = %workflow.name,
            version = workflow.version,
            "Updating workflow definition"
        );

        let _: serde_json::Value = self.api.put("/metadata/workflow", &[workflow]).await?;

        info!(
            workflow_name = %workflow.name,
            version = workflow.version,
            "Workflow definition updated"
        );

        Ok(())
    }

    /// Register or update a workflow definition
    pub async fn register_or_update_workflow_def(
        &self,
        workflow: &WorkflowDef,
        overwrite: bool,
    ) -> Result<()> {
        if overwrite {
            // Use PUT to update existing
            self.update_workflow_def(workflow).await
        } else {
            // Use POST to create new
            self.register_workflow_def(workflow).await
        }
    }

    /// Get a workflow definition by name and version
    pub async fn get_workflow_def(&self, name: &str, version: Option<i32>) -> Result<WorkflowDef> {
        let path = if let Some(v) = version {
            format!("/metadata/workflow/{}?version={}", name, v)
        } else {
            format!("/metadata/workflow/{}", name)
        };

        self.api.get(ApiPath::templated(&path, "/metadata/workflow/{name}")).await
    }

    /// Get all versions of a workflow definition
    pub async fn get_all_workflow_def_versions(&self, name: &str) -> Result<Vec<WorkflowDef>> {
        let path = format!("/metadata/workflow/{}/versions", name);
        self.api.get(ApiPath::templated(&path, "/metadata/workflow/{name}/versions")).await
    }

    /// Get all workflow definitions
    pub async fn get_all_workflow_defs(&self) -> Result<Vec<WorkflowDef>> {
        self.api.get("/metadata/workflow").await
    }

    /// Get all workflow definitions with latest versions (alias for get_all_workflow_defs)
    pub async fn get_all_workflow_defs_latest_versions(&self) -> Result<Vec<WorkflowDef>> {
        self.get_all_workflow_defs().await
    }

    /// Delete a workflow definition
    pub async fn delete_workflow_def(&self, name: &str, version: i32) -> Result<()> {
        let path = format!("/metadata/workflow/{}/{}", name, version);
        self.api.delete_no_content(ApiPath::templated(&path, "/metadata/workflow/{name}/{version}")).await
    }

    // ==================== Task Definitions ====================

    /// Register a new task definition
    pub async fn register_task_def(&self, task: &TaskDef) -> Result<()> {
        debug!(task_name = %task.name, "Registering task definition");

        let _: serde_json::Value = self.api.post("/metadata/taskdefs", &[task]).await?;

        info!(task_name = %task.name, "Task definition registered");

        Ok(())
    }

    /// Register multiple task definitions
    pub async fn register_task_defs(&self, tasks: &[TaskDef]) -> Result<()> {
        debug!(count = tasks.len(), "Registering task definitions");

        let _: serde_json::Value = self.api.post("/metadata/taskdefs", tasks).await?;

        info!(count = tasks.len(), "Task definitions registered");

        Ok(())
    }

    /// Update a task definition
    pub async fn update_task_def(&self, task: &TaskDef) -> Result<()> {
        debug!(task_name = %task.name, "Updating task definition");

        let _: serde_json::Value = self.api.put("/metadata/taskdefs", task).await?;

        info!(task_name = %task.name, "Task definition updated");

        Ok(())
    }

    /// Get a task definition by name
    pub async fn get_task_def(&self, name: &str) -> Result<TaskDef> {
        let path = format!("/metadata/taskdefs/{}", name);
        self.api.get(ApiPath::templated(&path, "/metadata/taskdefs/{name}")).await
    }

    /// Get all task definitions
    pub async fn get_all_task_defs(&self) -> Result<Vec<TaskDef>> {
        self.api.get("/metadata/taskdefs").await
    }

    /// Delete a task definition
    pub async fn delete_task_def(&self, name: &str) -> Result<()> {
        let path = format!("/metadata/taskdefs/{}", name);
        self.api.delete_no_content(ApiPath::templated(&path, "/metadata/taskdefs/{name}")).await
    }

    /// Check if a task definition exists
    pub async fn task_def_exists(&self, name: &str) -> Result<bool> {
        match self.get_task_def(name).await {
            Ok(_) => Ok(true),
            Err(crate::error::ConductorError::Api { .. }) => Ok(false),
            Err(crate::error::ConductorError::Server { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Check if a workflow definition exists
    pub async fn workflow_def_exists(&self, name: &str, version: Option<i32>) -> Result<bool> {
        match self.get_workflow_def(name, version).await {
            Ok(_) => Ok(true),
            Err(crate::error::ConductorError::Api { .. }) => Ok(false),
            Err(crate::error::ConductorError::Server { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RetryLogic, TimeoutPolicy};

    #[test]
    fn test_task_def_serialization() {
        let task = TaskDef::new("test_task")
            .with_description("A test task")
            .with_retry(3, RetryLogic::ExponentialBackoff, 5)
            .with_timeout(120, TimeoutPolicy::Retry);

        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("\"name\":\"test_task\""));
        assert!(json.contains("\"retryCount\":3"));
    }

    #[test]
    fn test_workflow_def_serialization() {
        use crate::models::WorkflowTask;

        let workflow = WorkflowDef::new("test_workflow")
            .with_description("A test workflow")
            .with_task(WorkflowTask::simple("task1", "task1_ref"));

        let json = serde_json::to_string(&workflow).unwrap();
        assert!(json.contains("\"name\":\"test_workflow\""));
        assert!(json.contains("\"tasks\":["));
    }
}
