// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! [`AiOrchestrator`] -- a thin typed convenience layer over [`IntegrationClient`]/
//! [`PromptClient`], for registering LLM/vector-DB integrations and testing prompt templates.
//!
//! Two fields are deliberately not present, since neither would actually be used by anything:
//! `prompt_test_workflow_name` (never read anywhere), and `workflow_client`/`workflow_executor`
//! (anyone who wants one already has [`crate::client::ConductorClient::workflow_client`]).

use std::collections::HashMap;

use serde_json::Value;

use crate::configuration::Configuration;
use crate::error::{ConductorError, Result};
use crate::http::ApiClient;
use crate::models::{
    IntegrationApiUpdate, IntegrationConfig, IntegrationUpdate, LlmProvider, PromptTemplate,
    VectorDb,
};

use super::{IntegrationClient, PromptClient};

/// Typed convenience layer for registering LLM/vector-DB integrations and testing prompt
/// templates, on top of [`IntegrationClient`]/[`PromptClient`].
#[derive(Clone)]
pub struct AiOrchestrator {
    integration_client: IntegrationClient,
    prompt_client: PromptClient,
}

impl AiOrchestrator {
    /// Create a new orchestrator from a [`Configuration`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the underlying `reqwest` client fails to build (see [`ApiClient::new`]).
    pub fn new(config: Configuration) -> Result<Self> {
        let api = ApiClient::new(config)?;
        Ok(Self::from_api_client(api))
    }

    /// Create a new orchestrator from an existing [`ApiClient`].
    #[must_use]
    pub fn from_api_client(api: ApiClient) -> Self {
        Self {
            integration_client: IntegrationClient::new(api.clone()),
            prompt_client: PromptClient::new(api),
        }
    }

    /// Create or update a prompt template.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn add_prompt_template(
        &self,
        name: &str,
        prompt_template: &str,
        description: &str,
    ) -> Result<()> {
        self.prompt_client
            .save_prompt(name, description, prompt_template)
            .await
    }

    /// Get a prompt template by name, or `None` if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status other than not-found, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_prompt_template(&self, template_name: &str) -> Result<Option<PromptTemplate>> {
        match self.prompt_client.get_prompt(template_name).await {
            Ok(template) => Ok(Some(template)),
            Err(ConductorError::Api { .. } | ConductorError::Server { status: 404, .. }) => {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Associate a prompt template with each of `ai_models` on `ai_integration`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if any request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status -- the first failure stops the loop, leaving any later models un-associated.
    pub async fn associate_prompt_template(
        &self,
        name: &str,
        ai_integration: &str,
        ai_models: &[String],
    ) -> Result<()> {
        for model in ai_models {
            self.integration_client
                .associate_prompt_with_integration(ai_integration, model, name)
                .await?;
        }
        Ok(())
    }

    /// Render and run a prompt template against a model, using defaults
    /// (`temperature = 0.0`, `top_p = 1.0`, no stop words). See
    /// [`AiOrchestrator::test_prompt_template_with_options`] to override them.
    ///
    /// # Errors
    ///
    /// Same as [`AiOrchestrator::test_prompt_template_with_options`].
    pub async fn test_prompt_template(
        &self,
        text: &str,
        variables: &HashMap<String, Value>,
        ai_integration: &str,
        text_complete_model: &str,
    ) -> Result<String> {
        self.test_prompt_template_with_options(
            text,
            variables,
            ai_integration,
            text_complete_model,
            None,
            0.0,
            1.0,
        )
        .await
    }

    /// [`AiOrchestrator::test_prompt_template`] with explicit `stop_words`/`temperature`/`top_p`.
    ///
    /// Deliberately has no `max_tokens` parameter: the server never forwards one for this
    /// endpoint, so accepting one here would be misleading about what the call actually does.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    #[expect(clippy::too_many_arguments)]
    pub async fn test_prompt_template_with_options(
        &self,
        text: &str,
        variables: &HashMap<String, Value>,
        ai_integration: &str,
        text_complete_model: &str,
        stop_words: Option<&[String]>,
        temperature: f32,
        top_p: f32,
    ) -> Result<String> {
        self.prompt_client
            .test_prompt(
                text,
                variables,
                ai_integration,
                text_complete_model,
                temperature,
                top_p,
                stop_words,
            )
            .await
    }

    /// Register (or update) an AI model integration and its models.
    ///
    /// If an integration named `name` already exists, it's left untouched unless `overwrite` is
    /// `true`; each model in `models` is likewise only (re-)registered if missing or `overwrite`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if any request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status other than not-found.
    pub async fn add_ai_integration(
        &self,
        name: &str,
        provider: LlmProvider,
        models: &[String],
        description: &str,
        config: &dyn IntegrationConfig,
        overwrite: bool,
    ) -> Result<()> {
        self.add_integration(
            name,
            provider.as_str(),
            "AI_MODEL",
            models,
            description,
            config,
            overwrite,
        )
        .await
    }

    /// Register (or update) a vector-DB integration and its indices.
    ///
    /// `description` defaults to `name` if omitted. See
    /// [`AiOrchestrator::add_ai_integration`] for the existence/`overwrite` semantics.
    ///
    /// # Errors
    ///
    /// Same as [`AiOrchestrator::add_ai_integration`].
    pub async fn add_vector_store(
        &self,
        name: &str,
        provider: VectorDb,
        indices: &[String],
        config: &dyn IntegrationConfig,
        description: Option<&str>,
        overwrite: bool,
    ) -> Result<()> {
        self.add_integration(
            name,
            provider.as_str(),
            "VECTOR_DB",
            indices,
            description.unwrap_or(name),
            config,
            overwrite,
        )
        .await
    }

    /// Shared upsert-integration-and-apis logic behind [`AiOrchestrator::add_ai_integration`]/
    /// [`AiOrchestrator::add_vector_store`] -- they differ only in `provider`'s enum type and
    /// `category`.
    #[expect(clippy::too_many_arguments)]
    async fn add_integration(
        &self,
        name: &str,
        provider: &str,
        category: &str,
        api_names: &[String],
        description: &str,
        config: &dyn IntegrationConfig,
        overwrite: bool,
    ) -> Result<()> {
        if !self.integration_exists(name).await? || overwrite {
            let details = IntegrationUpdate {
                integration_type: Some(provider.to_owned()),
                category: Some(category.to_owned()),
                enabled: Some(true),
                description: Some(description.to_owned()),
                configuration: config.to_config(),
            };
            self.integration_client
                .save_integration(name, &details)
                .await?;
        }

        for api_name in api_names {
            if !self.integration_api_exists(name, api_name).await? || overwrite {
                let api_details = IntegrationApiUpdate {
                    enabled: Some(true),
                    description: Some(description.to_owned()),
                    ..IntegrationApiUpdate::default()
                };
                self.integration_client
                    .save_integration_api(name, api_name, &api_details)
                    .await?;
            }
        }

        Ok(())
    }

    /// Same not-found-tolerant pattern as `MetadataClient::task_def_exists`/`workflow_def_exists`.
    async fn integration_exists(&self, name: &str) -> Result<bool> {
        match self.integration_client.get_integration(name).await {
            Ok(_) => Ok(true),
            Err(ConductorError::Api { .. } | ConductorError::Server { status: 404, .. }) => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    async fn integration_api_exists(&self, integration_name: &str, api_name: &str) -> Result<bool> {
        match self
            .integration_client
            .get_integration_api(api_name, integration_name)
            .await
        {
            Ok(_) => Ok(true),
            Err(ConductorError::Api { .. } | ConductorError::Server { status: 404, .. }) => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Total token usage recorded for an AI integration provider.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_token_used(&self, ai_integration: &str) -> Result<Value> {
        self.integration_client
            .get_token_usage_for_integration_provider(ai_integration)
            .await
    }

    /// Total token usage recorded for one model on an AI integration provider.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_token_used_by_model(&self, ai_integration: &str, model: &str) -> Result<i64> {
        self.integration_client
            .get_token_usage_for_integration(model, ai_integration)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_orchestrator_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let _orchestrator = AiOrchestrator::new(config).unwrap();
    }
}
