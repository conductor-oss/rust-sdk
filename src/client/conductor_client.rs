// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::configuration::Configuration;
use crate::error::Result;
use crate::http::ApiClient;

use super::{
    AuthorizationClient, EventClient, IntegrationClient, MetadataClient, OrkesMetadataClient,
    PromptClient, SchedulerClient, SchemaClient, SecretClient, TaskClient, WorkflowClient,
};

/// Main Conductor client combining all API clients
///
/// This is the primary entry point for interacting with the Conductor API.
/// Also available as `OrkesClients` alias for Python SDK compatibility.
#[derive(Clone)]
pub struct ConductorClient {
    api: ApiClient,
}

impl ConductorClient {
    /// Create a new Conductor client with the given configuration
    pub fn new(config: Configuration) -> Result<Self> {
        let api = ApiClient::new(config)?;
        Ok(Self { api })
    }

    /// Create from an existing API client
    pub fn from_api_client(api: ApiClient) -> Self {
        Self { api }
    }

    /// Get the task client for polling and updating tasks
    pub fn task_client(&self) -> TaskClient {
        TaskClient::new(self.api.clone())
    }

    /// Alias for task_client() - matches Python SDK naming
    pub fn get_task_client(&self) -> TaskClient {
        self.task_client()
    }

    /// Get the workflow client for workflow operations
    pub fn workflow_client(&self) -> WorkflowClient {
        WorkflowClient::new(self.api.clone())
    }

    /// Alias for workflow_client() - matches Python SDK naming
    pub fn get_workflow_client(&self) -> WorkflowClient {
        self.workflow_client()
    }

    /// Get the metadata client for managing definitions
    pub fn metadata_client(&self) -> MetadataClient {
        MetadataClient::new(self.api.clone())
    }

    /// Alias for metadata_client() - matches Python SDK naming
    pub fn get_metadata_client(&self) -> MetadataClient {
        self.metadata_client()
    }

    /// Get the Orkes metadata client with tagging APIs
    ///
    /// This client extends MetadataClient with Orkes-specific features
    /// like workflow and task tagging. Access base methods via Deref.
    pub fn orkes_metadata_client(&self) -> OrkesMetadataClient {
        OrkesMetadataClient::new(self.api.clone())
    }

    /// Alias for orkes_metadata_client() - matches Python SDK naming
    pub fn get_orkes_metadata_client(&self) -> OrkesMetadataClient {
        self.orkes_metadata_client()
    }

    /// Get the scheduler client for managing workflow schedules
    pub fn scheduler_client(&self) -> SchedulerClient {
        SchedulerClient::new(self.api.clone())
    }

    /// Alias for scheduler_client() - matches Python SDK naming
    pub fn get_scheduler_client(&self) -> SchedulerClient {
        self.scheduler_client()
    }

    /// Get the secret client for managing secrets
    pub fn secret_client(&self) -> SecretClient {
        SecretClient::new(self.api.clone())
    }

    /// Alias for secret_client() - matches Python SDK naming
    pub fn get_secret_client(&self) -> SecretClient {
        self.secret_client()
    }

    /// Get the authorization client for users, groups, and permissions
    pub fn authorization_client(&self) -> AuthorizationClient {
        AuthorizationClient::new(self.api.clone())
    }

    /// Alias for authorization_client() - matches Python SDK naming
    pub fn get_authorization_client(&self) -> AuthorizationClient {
        self.authorization_client()
    }

    /// Get the integration client for external system integrations
    pub fn integration_client(&self) -> IntegrationClient {
        IntegrationClient::new(self.api.clone())
    }

    /// Alias for integration_client() - matches Python SDK naming
    pub fn get_integration_client(&self) -> IntegrationClient {
        self.integration_client()
    }

    /// Get the prompt client for AI prompt templates
    pub fn prompt_client(&self) -> PromptClient {
        PromptClient::new(self.api.clone())
    }

    /// Alias for prompt_client() - matches Python SDK naming
    pub fn get_prompt_client(&self) -> PromptClient {
        self.prompt_client()
    }

    /// Get the schema client for schema definitions
    pub fn schema_client(&self) -> SchemaClient {
        SchemaClient::new(self.api.clone())
    }

    /// Alias for schema_client() - matches Python SDK naming
    pub fn get_schema_client(&self) -> SchemaClient {
        self.schema_client()
    }

    /// Get the event client for event queue configurations
    pub fn event_client(&self) -> EventClient {
        EventClient::new(self.api.clone())
    }

    /// Alias for event_client() - matches Python SDK naming
    pub fn get_event_client(&self) -> EventClient {
        self.event_client()
    }

    /// Get the underlying API client
    pub fn api_client(&self) -> &ApiClient {
        &self.api
    }

    /// Get configuration
    pub async fn config(&self) -> Configuration {
        self.api.get_config().await
    }
}

/// Builder for ConductorClient
pub struct ConductorClientBuilder {
    config: Configuration,
}

impl ConductorClientBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: Configuration::default(),
        }
    }

    /// Create a builder from an existing configuration
    pub fn from_config(config: Configuration) -> Self {
        Self { config }
    }

    /// Set the server URL
    pub fn server_url(mut self, url: impl Into<String>) -> Self {
        self.config.server_api_url = url.into();
        self
    }

    /// Set authentication credentials
    pub fn auth(mut self, key: impl Into<String>, secret: impl Into<String>) -> Self {
        self.config.auth_key = Some(key.into());
        self.config.auth_secret = Some(secret.into());
        self
    }

    /// Enable debug mode
    pub fn debug(mut self, enabled: bool) -> Self {
        self.config.debug = enabled;
        self
    }

    /// Build the client
    pub fn build(self) -> Result<ConductorClient> {
        ConductorClient::new(self.config)
    }
}

impl Default for ConductorClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let client = ConductorClientBuilder::new()
            .server_url("http://localhost:8080/api")
            .debug(true)
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let client = ConductorClient::new(config);
        assert!(client.is_ok());
    }
}
