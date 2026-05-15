// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
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
        self.api.post_no_body_no_response(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/prompt/{promptName}")).await
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
        self.api.delete_no_content(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}")).await
    }

    /// Delete an integration
    pub async fn delete_integration(&self, integration_name: &str) -> Result<()> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.delete_no_content(ApiPath::templated(&path, "/integrations/provider/{name}")).await
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
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}")).await
    }

    /// Get all APIs for an integration
    pub async fn get_integration_apis(
        &self,
        integration_name: &str,
    ) -> Result<Vec<IntegrationApi>> {
        let path = format!("/integrations/provider/{}/integration", integration_name);
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/integration")).await
    }

    /// Get an integration
    pub async fn get_integration(&self, integration_name: &str) -> Result<Integration> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}")).await
    }

    /// Get all integrations
    pub async fn get_integrations(&self) -> Result<Vec<Integration>> {
        self.api.get("/integrations/provider").await
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
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/prompt")).await
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
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/metrics")).await
    }

    /// Get token usage for an integration provider
    pub async fn get_token_usage_for_integration_provider(
        &self,
        name: &str,
    ) -> Result<serde_json::Value> {
        let path = format!("/integrations/provider/{}/metrics", name);
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/metrics")).await
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
        self.api.put_no_response(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}"), api_details).await
    }

    /// Save (create or update) an integration
    pub async fn save_integration(
        &self,
        integration_name: &str,
        integration_details: &IntegrationUpdate,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}", integration_name);
        self.api.put_no_response(ApiPath::templated(&path, "/integrations/provider/{name}"), integration_details).await
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
        self.api.delete_with_body(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/tags"), tags).await
    }

    /// Delete a tag from an integration provider
    pub async fn delete_tag_for_integration_provider(
        &self,
        tags: &[MetadataTag],
        name: &str,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.delete_with_body(ApiPath::templated(&path, "/integrations/provider/{name}/tags"), tags).await
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
        self.api.put_no_response(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/tags"), tags).await
    }

    /// Set tags for an integration provider
    pub async fn put_tag_for_integration_provider(
        &self,
        tags: &[MetadataTag],
        name: &str,
    ) -> Result<()> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.put_no_response(ApiPath::templated(&path, "/integrations/provider/{name}/tags"), tags).await
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
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/integration/{apiName}/tags")).await
    }

    /// Get tags for an integration provider
    pub async fn get_tags_for_integration_provider(&self, name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/integrations/provider/{}/tags", name);
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/tags")).await
    }

    /// Get available APIs for an integration provider
    pub async fn get_integration_available_apis(
        &self,
        integration_name: &str,
    ) -> Result<Vec<String>> {
        let path = format!("/integrations/provider/{}/models", integration_name);
        self.api.get(ApiPath::templated(&path, "/integrations/provider/{name}/models")).await
    }

    /// Get all integration provider definitions
    pub async fn get_integration_provider_defs(&self) -> Result<Vec<serde_json::Value>> {
        self.api.get("/integrations/def").await
    }

    /// Get all providers and their integrations
    pub async fn get_providers_and_integrations(&self) -> Result<serde_json::Value> {
        self.api.get("/integrations").await
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
