// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Prompt template
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    /// Prompt name
    pub name: String,

    /// Template text
    pub template: String,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Variables in the template
    #[serde(default)]
    pub variables: Vec<String>,

    /// Associated AI models
    #[serde(default)]
    pub models: Vec<String>,

    /// Template version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

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

impl PromptTemplate {
    /// Create a new prompt template
    pub fn new(name: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            template: template.into(),
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set models
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }
}

/// Request to test a prompt
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPromptRequest {
    /// Prompt text
    pub prompt_text: String,

    /// Variables to substitute
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// AI integration name
    pub ai_integration: String,

    /// Model name
    pub model: String,

    /// Temperature (0-1)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top P (0-1)
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Stop words
    #[serde(default)]
    pub stop_words: Vec<String>,
}

fn default_temperature() -> f32 {
    0.1
}

fn default_top_p() -> f32 {
    0.9
}
