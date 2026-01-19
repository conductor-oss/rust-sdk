//! Schema definition models

use serde::{Deserialize, Serialize};

/// Schema definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDef {
    /// Schema name
    pub name: String,

    /// Schema version
    pub version: i32,

    /// Schema type (e.g., "JSON")
    #[serde(rename = "type", default = "default_schema_type")]
    pub schema_type: String,

    /// The actual schema definition (JSON Schema)
    #[serde(default)]
    pub data: serde_json::Value,

    /// Whether this schema is used for external storage
    #[serde(default)]
    pub external_ref: bool,

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

fn default_schema_type() -> String {
    "JSON".to_string()
}

impl SchemaDef {
    /// Create a new schema definition
    pub fn new(name: impl Into<String>, version: i32, data: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            version,
            schema_type: default_schema_type(),
            data,
            ..Default::default()
        }
    }

    /// Create a JSON schema
    pub fn json_schema(name: impl Into<String>, version: i32, schema: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            version,
            schema_type: "JSON".to_string(),
            data: schema,
            ..Default::default()
        }
    }
}
