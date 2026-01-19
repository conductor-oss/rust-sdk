//! Integration client for managing external system integrations

use crate::error::Result;
use crate::http::ApiClient;
use crate::models::{
    Integration, IntegrationApi, IntegrationApiUpdate, IntegrationUpdate, MetadataTag,
    PromptTemplate,
};

/// Client for managing integrations with external systems
#[derive(Clone)]
pub struct IntegrationClient {
    api: ApiClient,
}

impl IntegrationClient {
    /// Create a new integration client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Associate a prompt with an AI integration and model
    pub async fn associate_prompt_with_integration(
        &self,
        ai_integration: &str,
        model_name: &str,
        prompt_name: &str,
    ) -> Result<()> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/prompt/{}",
            ai_integration, model_name, prompt_name
        );
        self.api.post_no_body_no_response(&path).await
    }

    /// Delete a specific integration API
    pub async fn delete_integration_api(
        &self,
        api_name: &str,
        integration_name: &str,
    ) -> Result<()> {
        let path = format!(
            "/integrations/provider/{}/integration/{}",
            integration_name, api_name
        );
        self.api.delete_no_content(&path).await
    }

    /// Delete an integration
    pub async fn delete_integration(&self, integration_name: &str) -> Result<()> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.delete_no_content(&path).await
    }

    /// Get an integration API
    pub async fn get_integration_api(
        &self,
        api_name: &str,
        integration_name: &str,
    ) -> Result<IntegrationApi> {
        let path = format!(
            "/integrations/provider/{}/integration/{}",
            integration_name, api_name
        );
        self.api.get(&path).await
    }

    /// Get all APIs for an integration
    pub async fn get_integration_apis(
        &self,
        integration_name: &str,
    ) -> Result<Vec<IntegrationApi>> {
        let path = format!("/integrations/provider/{}/integration", integration_name);
        self.api.get(&path).await
    }

    /// Get an integration
    pub async fn get_integration(&self, integration_name: &str) -> Result<Integration> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.get(&path).await
    }

    /// Get all integrations
    pub async fn get_integrations(&self) -> Result<Vec<Integration>> {
        let path = "/integrations/provider";
        self.api.get(path).await
    }

    /// Get prompts associated with an integration
    pub async fn get_prompts_with_integration(
        &self,
        ai_integration: &str,
        model_name: &str,
    ) -> Result<Vec<PromptTemplate>> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/prompt",
            ai_integration, model_name
        );
        self.api.get(&path).await
    }

    /// Get token usage for an integration API
    pub async fn get_token_usage_for_integration(
        &self,
        api_name: &str,
        integration_name: &str,
    ) -> Result<i64> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/metrics",
            integration_name, api_name
        );
        self.api.get(&path).await
    }

    /// Get token usage for an integration provider
    pub async fn get_token_usage_for_integration_provider(
        &self,
        name: &str,
    ) -> Result<serde_json::Value> {
        let path = format!("/integrations/provider/{}/metrics", name);
        self.api.get(&path).await
    }

    /// Save (create or update) an integration API
    pub async fn save_integration_api(
        &self,
        integration_name: &str,
        api_name: &str,
        api_details: &IntegrationApiUpdate,
    ) -> Result<()> {
        let path = format!(
            "/integrations/provider/{}/integration/{}",
            integration_name, api_name
        );
        self.api.put_no_response(&path, api_details).await
    }

    /// Save (create or update) an integration
    pub async fn save_integration(
        &self,
        integration_name: &str,
        integration_details: &IntegrationUpdate,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.put_no_response(&path, integration_details).await
    }

    // Tags

    /// Delete a tag from an integration
    pub async fn delete_tag_for_integration(
        &self,
        tags: &[MetadataTag],
        integration_name: &str,
        api_name: &str,
    ) -> Result<()> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/tags",
            integration_name, api_name
        );
        self.api.delete_with_body(&path, tags).await
    }

    /// Delete a tag from an integration provider
    pub async fn delete_tag_for_integration_provider(
        &self,
        tags: &[MetadataTag],
        name: &str,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.delete_with_body(&path, tags).await
    }

    /// Set tags for an integration
    pub async fn put_tag_for_integration(
        &self,
        tags: &[MetadataTag],
        integration_name: &str,
        api_name: &str,
    ) -> Result<()> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/tags",
            integration_name, api_name
        );
        self.api.put_no_response(&path, tags).await
    }

    /// Set tags for an integration provider
    pub async fn put_tag_for_integration_provider(
        &self,
        tags: &[MetadataTag],
        name: &str,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.put_no_response(&path, tags).await
    }

    /// Get tags for an integration
    pub async fn get_tags_for_integration(
        &self,
        integration_name: &str,
        api_name: &str,
    ) -> Result<Vec<MetadataTag>> {
        let path = format!(
            "/integrations/provider/{}/integration/{}/tags",
            integration_name, api_name
        );
        self.api.get(&path).await
    }

    /// Get tags for an integration provider
    pub async fn get_tags_for_integration_provider(&self, name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.get(&path).await
    }

    /// Get available APIs for an integration provider
    pub async fn get_integration_available_apis(
        &self,
        integration_name: &str,
    ) -> Result<Vec<String>> {
        let path = format!("/integrations/provider/{}/models", integration_name);
        self.api.get(&path).await
    }

    /// Get all integration provider definitions
    pub async fn get_integration_provider_defs(&self) -> Result<Vec<serde_json::Value>> {
        let path = "/integrations/def";
        self.api.get(path).await
    }

    /// Get all providers and their integrations
    pub async fn get_providers_and_integrations(&self) -> Result<serde_json::Value> {
        let path = "/integrations";
        self.api.get(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_integration_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = IntegrationClient::new(api);
    }
}
