//! RerunWorkflowRequest model

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request to rerun a workflow from a specific task
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RerunWorkflowRequest {
    /// The workflow ID to rerun from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub re_run_from_workflow_id: Option<String>,

    /// The task ID to rerun from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub re_run_from_task_id: Option<String>,

    /// Task input override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_input: Option<HashMap<String, serde_json::Value>>,

    /// Workflow input override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_input: Option<HashMap<String, serde_json::Value>>,

    /// Correlation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

impl RerunWorkflowRequest {
    /// Create a new rerun request from a workflow ID
    pub fn new(workflow_id: impl Into<String>) -> Self {
        Self {
            re_run_from_workflow_id: Some(workflow_id.into()),
            ..Default::default()
        }
    }

    /// Create from a task ID
    pub fn from_task_id(task_id: impl Into<String>) -> Self {
        Self {
            re_run_from_task_id: Some(task_id.into()),
            ..Default::default()
        }
    }

    /// Set task input
    pub fn with_task_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.task_input = Some(input);
        self
    }

    /// Set workflow input
    pub fn with_workflow_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.workflow_input = Some(input);
        self
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }
}
