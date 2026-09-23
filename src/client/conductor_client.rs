// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::configuration::Configuration;
use crate::error::Result;
use crate::events::EventDispatcher;
use crate::http::ApiClient;

#[cfg(feature = "agents")]
use super::AgentClient;
use super::{
    AuthorizationClient, EventClient, IntegrationClient, MetadataClient, OrkesMetadataClient,
    PromptClient, SchedulerClient, SchemaClient, SecretClient, TaskClient, WorkflowClient,
};

/// Main Conductor client combining all API clients.
///
/// This is the primary entry point for interacting with the Conductor API.
/// Also available as the `OrkesClients` alias.
#[derive(Clone)]
pub struct ConductorClient {
    api: ApiClient,
    /// Shared event dispatcher used by service clients that emit events
    /// (currently [`WorkflowClient`]). Defaults to an empty dispatcher;
    /// replace with [`Self::with_event_dispatcher`] to wire up listeners
    /// such as the metrics collector.
    events: EventDispatcher,
}

impl ConductorClient {
    /// Create a new Conductor client with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the underlying `reqwest` client fails to build
    /// (e.g. TLS backend initialization failure) -- this doesn't make any network request.
    pub fn new(config: Configuration) -> Result<Self> {
        let api = ApiClient::new(config)?;
        Ok(Self {
            api,
            events: EventDispatcher::default(),
        })
    }

    /// Create from an existing API client.
    #[must_use]
    pub fn from_api_client(api: ApiClient) -> Self {
        Self {
            api,
            events: EventDispatcher::default(),
        }
    }

    /// Share an [`EventDispatcher`] with this client so that service clients
    /// (such as [`WorkflowClient`]) publish events to it.
    ///
    /// Typically used to route workflow-lifecycle events through the same
    /// dispatcher as [`TaskHandler`](crate::worker::TaskHandler), allowing a
    /// single `MetricsCollector` to observe both task- and workflow-level
    /// metrics.
    #[must_use]
    pub fn with_event_dispatcher(mut self, events: EventDispatcher) -> Self {
        self.events = events;
        self
    }

    /// Access the shared event dispatcher.
    #[must_use]
    pub fn event_dispatcher(&self) -> &EventDispatcher {
        &self.events
    }

    /// Get the task client for polling and updating tasks.
    #[must_use]
    pub fn task_client(&self) -> TaskClient {
        TaskClient::new(self.api.clone())
    }

    /// Alias for `task_client()`.
    #[must_use]
    pub fn get_task_client(&self) -> TaskClient {
        self.task_client()
    }

    /// Get the workflow client for workflow operations.
    #[must_use]
    pub fn workflow_client(&self) -> WorkflowClient {
        WorkflowClient::new_with_events(self.api.clone(), self.events.clone())
    }

    /// Alias for `workflow_client()`.
    #[must_use]
    pub fn get_workflow_client(&self) -> WorkflowClient {
        self.workflow_client()
    }

    /// Get the metadata client for managing definitions.
    #[must_use]
    pub fn metadata_client(&self) -> MetadataClient {
        MetadataClient::new(self.api.clone())
    }

    /// Alias for `metadata_client()`.
    #[must_use]
    pub fn get_metadata_client(&self) -> MetadataClient {
        self.metadata_client()
    }

    /// Get the Orkes metadata client with tagging APIs.
    ///
    /// This client extends `MetadataClient` with Orkes-specific features
    /// like workflow and task tagging. Access base methods via Deref.
    #[must_use]
    pub fn orkes_metadata_client(&self) -> OrkesMetadataClient {
        OrkesMetadataClient::new(self.api.clone())
    }

    /// Alias for `orkes_metadata_client()`.
    #[must_use]
    pub fn get_orkes_metadata_client(&self) -> OrkesMetadataClient {
        self.orkes_metadata_client()
    }

    /// Get the scheduler client for managing workflow schedules.
    #[must_use]
    pub fn scheduler_client(&self) -> SchedulerClient {
        SchedulerClient::new(self.api.clone())
    }

    /// Alias for `scheduler_client()`.
    #[must_use]
    pub fn get_scheduler_client(&self) -> SchedulerClient {
        self.scheduler_client()
    }

    /// Get the secret client for managing secrets.
    #[must_use]
    pub fn secret_client(&self) -> SecretClient {
        SecretClient::new(self.api.clone())
    }

    /// Alias for `secret_client()`.
    #[must_use]
    pub fn get_secret_client(&self) -> SecretClient {
        self.secret_client()
    }

    /// Get the authorization client for users, groups, and permissions.
    #[must_use]
    pub fn authorization_client(&self) -> AuthorizationClient {
        AuthorizationClient::new(self.api.clone())
    }

    /// Alias for `authorization_client()`.
    #[must_use]
    pub fn get_authorization_client(&self) -> AuthorizationClient {
        self.authorization_client()
    }

    /// Get the integration client for external system integrations.
    #[must_use]
    pub fn integration_client(&self) -> IntegrationClient {
        IntegrationClient::new(self.api.clone())
    }

    /// Alias for `integration_client()`.
    #[must_use]
    pub fn get_integration_client(&self) -> IntegrationClient {
        self.integration_client()
    }

    /// Get the prompt client for AI prompt templates.
    #[must_use]
    pub fn prompt_client(&self) -> PromptClient {
        PromptClient::new(self.api.clone())
    }

    /// Alias for `prompt_client()`.
    #[must_use]
    pub fn get_prompt_client(&self) -> PromptClient {
        self.prompt_client()
    }

    /// Get the schema client for schema definitions.
    #[must_use]
    pub fn schema_client(&self) -> SchemaClient {
        SchemaClient::new(self.api.clone())
    }

    /// Alias for `schema_client()`.
    #[must_use]
    pub fn get_schema_client(&self) -> SchemaClient {
        self.schema_client()
    }

    /// Get the event client for event queue configurations.
    #[must_use]
    pub fn event_client(&self) -> EventClient {
        EventClient::new(self.api.clone())
    }

    /// Alias for `event_client()`.
    #[must_use]
    pub fn get_event_client(&self) -> EventClient {
        self.event_client()
    }

    /// Get the agent client for the Agent Runtime control-plane API (`/agent/*`).
    ///
    /// Requires the `agents` Cargo feature.
    #[cfg(feature = "agents")]
    #[must_use]
    pub fn agent_client(&self) -> AgentClient {
        AgentClient::new(self.api.clone())
    }

    /// Alias for `agent_client()`.
    #[cfg(feature = "agents")]
    #[must_use]
    pub fn get_agent_client(&self) -> AgentClient {
        self.agent_client()
    }

    /// Get the underlying API client.
    #[must_use]
    pub fn api_client(&self) -> &ApiClient {
        &self.api
    }

    /// Check whether the server is OSS Conductor (vs Orkes Enterprise).
    ///
    /// The result is cached after the first call. See [`ApiClient::is_oss`] for
    /// details on the detection mechanism.
    pub async fn is_oss(&self) -> bool {
        self.api.is_oss().await
    }

    /// Get configuration.
    pub async fn config(&self) -> Configuration {
        self.api.get_config().await
    }
}

/// Builder for `ConductorClient`.
pub struct ConductorClientBuilder {
    config: Configuration,
}

impl ConductorClientBuilder {
    /// Create a new builder with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: Configuration::default(),
        }
    }

    /// Create a builder from an existing configuration.
    #[must_use]
    pub fn from_config(config: Configuration) -> Self {
        Self { config }
    }

    /// Set the server URL.
    #[must_use]
    pub fn server_url(mut self, url: impl Into<String>) -> Self {
        self.config.server_api_url = url.into();
        self
    }

    /// Set authentication credentials.
    #[must_use]
    pub fn auth(mut self, key: impl Into<String>, secret: impl Into<String>) -> Self {
        self.config.auth_key = Some(key.into());
        self.config.auth_secret = Some(secret.into());
        self
    }

    /// Enable debug mode.
    #[must_use]
    pub fn debug(mut self, enabled: bool) -> Self {
        self.config.debug = enabled;
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
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

        client.unwrap();
    }

    #[test]
    fn test_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let client = ConductorClient::new(config);
        client.unwrap();
    }
}
