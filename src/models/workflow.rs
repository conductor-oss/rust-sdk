//! Workflow execution model

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::Task;

/// Module for flexible timestamp deserialization (handles both i64 and ISO date strings)
mod timestamp_deserializer {
    use serde::{self, Deserialize, Deserializer};
    use chrono::{DateTime, Utc};

    /// Deserialize a timestamp that may be either i64 (epoch ms) or ISO 8601 string
    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TimestampOrString {
            Timestamp(i64),
            String(String),
        }

        match TimestampOrString::deserialize(deserializer)? {
            TimestampOrString::Timestamp(ts) => Ok(ts),
            TimestampOrString::String(s) => {
                // Try to parse as ISO 8601 date string
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc).timestamp_millis())
                    .or_else(|_| {
                        // Try alternative formats
                        s.parse::<i64>()
                    })
                    .map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Module for flexible map deserialization (handles both maps and string representations)
mod flexible_map_deserializer {
    use serde::{self, Deserialize, Deserializer};
    use std::collections::HashMap;

    /// Deserialize a map that may be either a proper JSON map or a string representation
    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, serde_json::Value>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MapOrString {
            Map(HashMap<String, serde_json::Value>),
            #[allow(dead_code)]
            String(String),
        }

        match MapOrString::deserialize(deserializer)? {
            MapOrString::Map(map) => Ok(map),
            MapOrString::String(_) => {
                // Return empty map if it's a string representation
                // The string is usually something like "{input=test_value}" which is not valid JSON
                Ok(HashMap::new())
            }
        }
    }
}

/// Module for flexible Vec<Task> deserialization (handles both arrays and string representations)
mod flexible_tasks_deserializer {
    use serde::{self, Deserialize, Deserializer};
    use super::Task;

    /// Deserialize tasks that may be either a proper array or a string representation
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Task>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum VecOrString {
            Vec(Vec<Task>),
            #[allow(dead_code)]
            String(String),
        }

        match VecOrString::deserialize(deserializer)? {
            VecOrString::Vec(vec) => Ok(vec),
            VecOrString::String(_) => {
                // Return empty vec if it's a string representation from search API
                Ok(Vec::new())
            }
        }
    }
}

/// Module for flexible Vec<String> deserialization (handles both arrays and string representations)
mod flexible_string_vec_deserializer {
    use serde::{self, Deserialize, Deserializer};

    /// Deserialize string vec that may be either a proper array or a string representation
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum VecOrString {
            Vec(Vec<String>),
            Single(String),
        }

        match VecOrString::deserialize(deserializer)? {
            VecOrString::Vec(vec) => Ok(vec),
            VecOrString::Single(s) => {
                // Return single-element vec if it's a string from search API
                if s.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![s])
                }
            }
        }
    }
}

/// Module for flexible HashMap<String, String> deserialization
mod flexible_string_map_deserializer {
    use serde::{self, Deserialize, Deserializer};
    use std::collections::HashMap;

    /// Deserialize string map that may be either a proper map or a string representation
    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum MapOrString {
            Map(HashMap<String, String>),
            #[allow(dead_code)]
            String(String),
        }

        match MapOrString::deserialize(deserializer)? {
            MapOrString::Map(map) => Ok(map),
            MapOrString::String(_) => {
                // Return empty map if it's a string representation from search API
                Ok(HashMap::new())
            }
        }
    }
}

/// Workflow execution status
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowStatus {
    /// Workflow is running
    #[default]
    Running,
    /// Workflow completed successfully
    Completed,
    /// Workflow failed
    Failed,
    /// Workflow timed out
    TimedOut,
    /// Workflow was terminated
    Terminated,
    /// Workflow is paused
    Paused,
}

/// A workflow execution instance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    /// Unique workflow ID
    #[serde(default)]
    pub workflow_id: String,

    /// Workflow type/name
    #[serde(default)]
    pub workflow_name: String,

    /// Workflow version
    #[serde(default)]
    pub workflow_version: i32,

    /// Workflow status
    #[serde(default)]
    pub status: WorkflowStatus,

    /// Workflow input (may be empty if returned as string from search API)
    #[serde(default, deserialize_with = "flexible_map_deserializer::deserialize")]
    pub input: HashMap<String, serde_json::Value>,

    /// Workflow output (may be empty if returned as string from search API)
    #[serde(default, deserialize_with = "flexible_map_deserializer::deserialize")]
    pub output: HashMap<String, serde_json::Value>,

    /// Correlation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Tasks in this workflow execution (may be empty from search API)
    #[serde(default, deserialize_with = "flexible_tasks_deserializer::deserialize")]
    pub tasks: Vec<Task>,

    /// Parent workflow ID (if this is a subworkflow)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_workflow_id: Option<String>,

    /// Parent workflow task ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_workflow_task_id: Option<String>,

    /// Start time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub start_time: i64,

    /// End time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub end_time: i64,

    /// Update time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub update_time: i64,

    /// Created time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub create_time: i64,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Updated by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,

    /// Reason for incompletion (if failed/terminated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_for_incompletion: Option<String>,

    /// Workflow variables
    #[serde(default, deserialize_with = "flexible_map_deserializer::deserialize")]
    pub variables: HashMap<String, serde_json::Value>,

    /// External input payload storage path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_input_payload_storage_path: Option<String>,

    /// External output payload storage path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_output_payload_storage_path: Option<String>,

    /// Priority
    #[serde(default)]
    pub priority: i32,

    /// Owner app
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_app: Option<String>,

    /// Task to domain mapping
    #[serde(default, deserialize_with = "flexible_string_map_deserializer::deserialize")]
    pub task_to_domain: HashMap<String, String>,

    /// Workflow definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_definition: Option<super::WorkflowDef>,

    /// Failed reference task names
    #[serde(default, deserialize_with = "flexible_string_vec_deserializer::deserialize")]
    pub failed_reference_task_names: Vec<String>,

    /// Failed task names
    #[serde(default, deserialize_with = "flexible_string_vec_deserializer::deserialize")]
    pub failed_task_names: Vec<String>,

    /// Last retried time (epoch ms or ISO 8601 string)
    #[serde(default, deserialize_with = "timestamp_deserializer::deserialize")]
    pub last_retried_time: i64,

    /// Event
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
}

impl Workflow {
    /// Check if workflow is terminal (completed, failed, terminated, timed out)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            WorkflowStatus::Completed
                | WorkflowStatus::Failed
                | WorkflowStatus::Terminated
                | WorkflowStatus::TimedOut
        )
    }

    /// Check if workflow is running (not terminal)
    pub fn is_running(&self) -> bool {
        !self.is_terminal()
    }

    /// Check if workflow succeeded
    pub fn is_successful(&self) -> bool {
        self.status == WorkflowStatus::Completed
    }

    /// Get an output value
    pub fn get_output<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.output
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get output as string
    pub fn get_output_string(&self, key: &str) -> Option<String> {
        self.output.get(key).map(|v| {
            if let serde_json::Value::String(s) = v {
                s.clone()
            } else {
                v.to_string()
            }
        })
    }

    /// Get a variable value
    pub fn get_variable<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.variables
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Request to start a workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorkflowRequest {
    /// Workflow name
    pub name: String,

    /// Workflow version (optional, uses latest if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    /// Workflow input
    #[serde(default)]
    pub input: HashMap<String, serde_json::Value>,

    /// Correlation ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Task to domain mapping
    #[serde(default)]
    pub task_to_domain: HashMap<String, String>,

    /// Workflow definition (for dynamic workflows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_def: Option<super::WorkflowDef>,

    /// External input payload storage path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_input_payload_storage_path: Option<String>,

    /// Priority
    #[serde(default)]
    pub priority: i32,
}

impl StartWorkflowRequest {
    /// Create a new start workflow request
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set workflow version
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set workflow input
    pub fn with_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.input = input;
        self
    }

    /// Add a single input value
    pub fn with_input_value(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.input.insert(
            key.into(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set task to domain mapping
    pub fn with_task_to_domain(mut self, mapping: HashMap<String, String>) -> Self {
        self.task_to_domain = mapping;
        self
    }

    /// Set workflow definition (for dynamic workflows)
    pub fn with_workflow_def(mut self, def: super::WorkflowDef) -> Self {
        self.workflow_def = Some(def);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_deserialization() {
        let json = r#"{
            "workflowId": "wf-123",
            "workflowName": "test_workflow",
            "status": "RUNNING",
            "input": {"key": "value"}
        }"#;

        let wf: Workflow = serde_json::from_str(json).unwrap();
        assert_eq!(wf.workflow_id, "wf-123");
        assert_eq!(wf.workflow_name, "test_workflow");
        assert_eq!(wf.status, WorkflowStatus::Running);
    }

    #[test]
    fn test_workflow_is_terminal() {
        let wf = Workflow {
            status: WorkflowStatus::Running,
            ..Default::default()
        };
        assert!(!wf.is_terminal());

        let wf = Workflow {
            status: WorkflowStatus::Completed,
            ..Default::default()
        };
        assert!(wf.is_terminal());
        assert!(wf.is_successful());

        let wf = Workflow {
            status: WorkflowStatus::Failed,
            ..Default::default()
        };
        assert!(wf.is_terminal());
        assert!(!wf.is_successful());
    }

    #[test]
    fn test_start_workflow_request() {
        let req = StartWorkflowRequest::new("my_workflow")
            .with_version(1)
            .with_input_value("name", "test")
            .with_correlation_id("corr-123");

        assert_eq!(req.name, "my_workflow");
        assert_eq!(req.version, Some(1));
        assert!(req.input.contains_key("name"));
        assert_eq!(req.correlation_id, Some("corr-123".to_string()));
    }
}
