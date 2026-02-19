// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::ApiClient;
use crate::models::{MetadataTag, PromptTemplate};
use std::collections::HashMap;

/// Client for managing AI prompt templates
#[derive(Clone)]
pub struct PromptClient {
    api: ApiClient,
}

impl PromptClient {
    /// Create a new prompt client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    // ==================== Basic CRUD operations ====================

    /// Creates or updates a prompt template (simplified method)
    /// This creates a new version or updates the latest version.
    pub async fn save_prompt(
        &self,
        prompt_name: &str,
        description: &str,
        prompt_template: &str,
    ) -> Result<()> {
        self.save_prompt_with_options(prompt_name, description, prompt_template, None, None, false)
            .await
    }

    /// Creates or updates a prompt template with full control over versioning and model associations
    ///
    /// # Arguments
    /// * `prompt_name` - The name of the prompt template
    /// * `description` - A description of what the prompt does
    /// * `prompt_template` - The template content with optional variable placeholders
    /// * `models` - Optional list of AI model names this prompt is compatible with (e.g., ["openai:gpt-4", "anthropic:sonnet-4.5"])
    /// * `version` - Specific version number to create or update, or None to update the latest version
    /// * `auto_increment` - If true, automatically creates a new version instead of updating existing
    pub async fn save_prompt_with_options(
        &self,
        prompt_name: &str,
        description: &str,
        prompt_template: &str,
        models: Option<&[String]>,
        version: Option<i32>,
        auto_increment: bool,
    ) -> Result<()> {
        let path = format!("/prompts/{}", prompt_name);

        let mut params: Vec<(&str, String)> = vec![("description", description.to_string())];

        if let Some(v) = version {
            params.push(("version", v.to_string()));
        }

        if auto_increment {
            params.push(("autoIncrement", "true".to_string()));
        }

        if let Some(m) = models {
            for model in m {
                params.push(("models", model.clone()));
            }
        }

        let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api
            .post_raw_with_params(&path, prompt_template, &params_ref)
            .await
    }

    /// Updates an existing prompt template at a specific version
    ///
    /// # Arguments
    /// * `prompt_name` - The name of the prompt template
    /// * `version` - The version number to update
    /// * `description` - A description of what the prompt does
    /// * `prompt_template` - The template content with optional variable placeholders
    /// * `models` - Optional list of AI model names this prompt is compatible with
    pub async fn update_prompt(
        &self,
        prompt_name: &str,
        version: i32,
        description: &str,
        prompt_template: &str,
        models: Option<&[String]>,
    ) -> Result<()> {
        let path = format!("/prompts/{}/{}", prompt_name, version);

        let mut params: Vec<(&str, String)> = vec![("description", description.to_string())];

        if let Some(m) = models {
            for model in m {
                params.push(("models", model.clone()));
            }
        }

        let params_ref: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.api
            .put_raw_with_params(&path, prompt_template, &params_ref)
            .await
    }

    /// Creates multiple prompt templates in a single bulk operation
    ///
    /// # Arguments
    /// * `prompts` - List of prompt templates to create
    /// * `new_version` - If true, creates new versions for existing prompts; if false, updates existing versions
    pub async fn save_prompts(&self, prompts: &[PromptTemplate], new_version: bool) -> Result<()> {
        let path = format!("/prompts?newVersion={}", new_version);
        let _: serde_json::Value = self.api.post(&path, prompts).await?;
        Ok(())
    }

    /// Retrieves the latest version of a prompt template by name
    pub async fn get_prompt(&self, prompt_name: &str) -> Result<PromptTemplate> {
        let path = format!("/prompts/{}", prompt_name);
        self.api.get(&path).await
    }

    /// Retrieves a specific version of a prompt template
    ///
    /// # Arguments
    /// * `prompt_name` - The name of the prompt template
    /// * `version` - The version number to retrieve
    pub async fn get_prompt_version(
        &self,
        prompt_name: &str,
        version: i32,
    ) -> Result<PromptTemplate> {
        let path = format!("/prompts/{}/{}", prompt_name, version);
        self.api.get(&path).await
    }

    /// Retrieves all versions of a specific prompt template
    ///
    /// # Arguments
    /// * `prompt_name` - The name of the prompt template
    ///
    /// # Returns
    /// List of all versions of the prompt template, ordered by version number
    pub async fn get_all_prompt_versions(&self, prompt_name: &str) -> Result<Vec<PromptTemplate>> {
        let path = format!("/prompts/{}/versions", prompt_name);
        self.api.get(&path).await
    }

    /// Retrieves all prompt templates (latest versions only)
    pub async fn get_prompts(&self) -> Result<Vec<PromptTemplate>> {
        let path = "/prompts";
        self.api.get(path).await
    }

    /// Deletes all versions of a prompt template
    pub async fn delete_prompt(&self, prompt_name: &str) -> Result<()> {
        let path = format!("/prompts/{}", prompt_name);
        self.api.delete_no_content(&path).await
    }

    /// Deletes a specific version of a prompt template
    ///
    /// # Arguments
    /// * `prompt_name` - The name of the prompt template
    /// * `version` - The version number to delete
    pub async fn delete_prompt_version(&self, prompt_name: &str, version: i32) -> Result<()> {
        let path = format!("/prompts/{}/{}", prompt_name, version);
        self.api.delete_no_content(&path).await
    }

    // ==================== Tag management ====================

    /// Retrieves all tags associated with a prompt template
    pub async fn get_tags_for_prompt_template(
        &self,
        prompt_name: &str,
    ) -> Result<Vec<MetadataTag>> {
        let path = format!("/prompts/{}/tags", prompt_name);
        self.api.get(&path).await
    }

    /// Adds or updates tags for a prompt template
    pub async fn update_tag_for_prompt_template(
        &self,
        prompt_name: &str,
        tags: &[MetadataTag],
    ) -> Result<()> {
        let path = format!("/prompts/{}/tags", prompt_name);
        self.api.put_no_response(&path, tags).await
    }

    /// Deletes specific tags from a prompt template
    pub async fn delete_tag_for_prompt_template(
        &self,
        prompt_name: &str,
        tags: &[MetadataTag],
    ) -> Result<()> {
        let path = format!("/prompts/{}/tags", prompt_name);
        self.api.delete_with_body(&path, tags).await
    }

    // ==================== Testing ====================

    /// Tests a prompt template by substituting variables and processing through the specified AI model
    ///
    /// This allows you to validate prompt templates before saving them or using them in workflows.
    ///
    /// # Arguments
    /// * `prompt_text` - The text of the prompt template with variable placeholders
    /// * `variables` - A map containing variables to be replaced in the template
    /// * `ai_integration` - The name of the AI integration provider (e.g., "openai")
    /// * `text_complete_model` - The AI model to use for completing text (e.g., "gpt-4")
    /// * `temperature` - The randomness of the output, typically 0.0 to 1.0 (default is 0.1)
    /// * `top_p` - The nucleus sampling probability mass (default is 0.9)
    /// * `stop_words` - Optional list of words at which to stop generating further text
    ///
    /// # Returns
    /// The processed prompt text after variable substitution and AI model processing
    #[allow(clippy::too_many_arguments)]
    pub async fn test_prompt(
        &self,
        prompt_text: &str,
        variables: &HashMap<String, serde_json::Value>,
        ai_integration: &str,
        text_complete_model: &str,
        temperature: f32,
        top_p: f32,
        stop_words: Option<&[String]>,
    ) -> Result<String> {
        let path = "/prompts/test";

        let mut body = serde_json::json!({
            "prompt": prompt_text,
            "promptVariables": variables,
            "llmProvider": ai_integration,
            "model": text_complete_model,
            "temperature": temperature,
            "topP": top_p
        });

        if let Some(sw) = stop_words {
            body["stopWords"] = serde_json::json!(sw);
        }

        self.api.post(path, &body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_prompt_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = PromptClient::new(api);
    }
}
