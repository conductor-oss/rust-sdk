// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};

/// Metadata tag for resources
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataTag {
    /// Tag key
    pub key: String,

    /// Tag type
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tag_type: Option<String>,

    /// Tag value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl MetadataTag {
    /// Create a new metadata tag
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            tag_type: None,
            value: None,
        }
    }

    /// Create a tag with key and value
    pub fn with_value(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            tag_type: None,
            value: Some(value.into()),
        }
    }

    /// Set the tag type
    pub fn with_type(mut self, tag_type: impl Into<String>) -> Self {
        self.tag_type = Some(tag_type.into());
        self
    }
}
