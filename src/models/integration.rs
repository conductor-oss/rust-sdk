//! Integration models for external systems (AI, Vector DBs, Kafka, etc.)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Integration with an external system
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    /// Integration name
    pub name: String,

    /// Integration type/category
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub integration_type: Option<String>,

    /// Provider category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Whether the integration is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Configuration
    #[serde(default)]
    pub configuration: HashMap<String, serde_json::Value>,

    /// Create time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Updated by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Integration API (model, index, topic, etc.)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationApi {
    /// API name
    pub name: String,

    /// Integration name this API belongs to
    pub integration_name: String,

    /// API configuration
    #[serde(default)]
    pub configuration: HashMap<String, serde_json::Value>,

    /// Whether the API is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Create time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Updated by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Request to update an integration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationUpdate {
    /// Integration type/category
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub integration_type: Option<String>,

    /// Category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Whether enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Configuration
    #[serde(default)]
    pub configuration: HashMap<String, serde_json::Value>,
}

/// Request to update an integration API
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationApiUpdate {
    /// Configuration
    #[serde(default)]
    pub configuration: HashMap<String, serde_json::Value>,

    /// Whether enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
