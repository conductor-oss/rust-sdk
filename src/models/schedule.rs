//! Workflow schedule models

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Workflow schedule definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSchedule {
    /// Schedule name (unique identifier)
    pub name: String,

    /// Cron expression for the schedule
    pub cron_expression: String,

    /// Workflow name to execute
    pub workflow_name: String,

    /// Workflow version to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<i32>,

    /// Start workflow request parameters
    #[serde(default)]
    pub start_workflow_request: Option<StartWorkflowScheduleRequest>,

    /// Whether the schedule is paused
    #[serde(default)]
    pub paused: bool,

    /// Whether to run tasks that were missed during the paused period
    #[serde(default)]
    pub run_catchup_schedule_instances: bool,

    /// Schedule start time (epoch ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_start_time: Option<i64>,

    /// Schedule end time (epoch ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_end_time: Option<i64>,

    /// Timezone for the cron expression
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,

    /// Create time (epoch ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Created by user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Update time (epoch ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Updated by user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Start workflow request for schedules
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkflowScheduleRequest {
    /// Workflow name
    pub name: String,

    /// Workflow version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    /// Correlation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Input parameters
    #[serde(default)]
    pub input: HashMap<String, serde_json::Value>,

    /// Task to domain mapping
    #[serde(default)]
    pub task_to_domain: HashMap<String, String>,

    /// Idempotency key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,

    /// Idempotency strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_strategy: Option<String>,
}

/// Request to save a schedule
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveScheduleRequest {
    /// Schedule name
    pub name: String,

    /// Cron expression
    pub cron_expression: String,

    /// Start workflow request
    pub start_workflow_request: StartWorkflowScheduleRequest,

    /// Whether the schedule is paused
    #[serde(default)]
    pub paused: bool,

    /// Run catchup schedule instances
    #[serde(default)]
    pub run_catchup_schedule_instances: bool,

    /// Schedule start time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_start_time: Option<i64>,

    /// Schedule end time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_end_time: Option<i64>,

    /// Timezone
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
}

impl SaveScheduleRequest {
    /// Create a new save schedule request
    pub fn new(
        name: impl Into<String>,
        cron_expression: impl Into<String>,
        workflow_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            cron_expression: cron_expression.into(),
            start_workflow_request: StartWorkflowScheduleRequest {
                name: workflow_name.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Set workflow version
    pub fn with_version(mut self, version: i32) -> Self {
        self.start_workflow_request.version = Some(version);
        self
    }

    /// Set workflow input
    pub fn with_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.start_workflow_request.input = input;
        self
    }

    /// Set timezone
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.zone_id = Some(timezone.into());
        self
    }

    /// Set paused state
    pub fn paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }
}

/// Schedule execution record
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowScheduleExecution {
    /// Execution ID
    #[serde(default)]
    pub execution_id: String,

    /// Schedule name
    #[serde(default)]
    pub schedule_name: String,

    /// Scheduled time
    #[serde(default)]
    pub scheduled_time: i64,

    /// Execution time
    #[serde(default)]
    pub execution_time: i64,

    /// Workflow ID that was started
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,

    /// Workflow name
    #[serde(default)]
    pub workflow_name: String,

    /// State of the execution
    #[serde(default)]
    pub state: String,

    /// Reason for failure (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Search result for schedule executions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultWorkflowScheduleExecution {
    /// Total number of hits
    #[serde(default)]
    pub total_hits: i64,

    /// Results
    #[serde(default)]
    pub results: Vec<WorkflowScheduleExecution>,
}
