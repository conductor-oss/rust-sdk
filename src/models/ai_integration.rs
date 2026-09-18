// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! LLM/vector-DB provider identifiers and their `configuration` payloads, for
//! [`crate::client::AiOrchestrator`]. Ports python's `conductor.client.ai.configuration`
//! (`LLMProvider`/`VectorDB`) and `conductor.client.ai.integrations` (`IntegrationConfig` and
//! its concrete subclasses).
//!
//! [`LlmProvider`]/[`VectorDb`] deliberately don't derive `Serialize`/`Deserialize`: they're
//! only ever written into an [`crate::models::IntegrationUpdate::integration_type`] string, never
//! read back off the wire, so [`LlmProvider::as_str`]/[`VectorDb::as_str`] (a plain match, same
//! pattern as `task_client.rs`'s `status_to_string`) is all that's needed -- adding a serde derive
//! here would mean picking a `rename_all` strategy that doesn't actually fit (the real values mix
//! case conventions, e.g. `Grok` is capitalized where every other [`LlmProvider`] value isn't).

use std::collections::HashMap;

use serde_json::Value;

/// An LLM provider identifier, for [`crate::client::AiOrchestrator::add_ai_integration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    /// Azure `OpenAI`.
    AzureOpenAi,
    /// `OpenAI`.
    OpenAi,
    /// GCP Vertex AI.
    GcpVertexAi,
    /// Hugging Face.
    HuggingFace,
    /// Anthropic.
    Anthropic,
    /// AWS Bedrock.
    Bedrock,
    /// Cohere.
    Cohere,
    /// Grok (xAI).
    Grok,
    /// Mistral.
    Mistral,
    /// Ollama.
    Ollama,
    /// Perplexity.
    Perplexity,
}

impl LlmProvider {
    /// The exact wire string the server's integration-provider registry expects, matching
    /// python's `LLMProvider` enum values byte-for-byte (including `Grok`'s inconsistent casing).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AzureOpenAi => "azure_openai",
            Self::OpenAi => "openai",
            Self::GcpVertexAi => "vertex_ai",
            Self::HuggingFace => "huggingface",
            Self::Anthropic => "anthropic",
            Self::Bedrock => "bedrock",
            Self::Cohere => "cohere",
            Self::Grok => "Grok",
            Self::Mistral => "mistral",
            Self::Ollama => "ollama",
            Self::Perplexity => "perplexity",
        }
    }
}

/// A vector database provider identifier, for [`crate::client::AiOrchestrator::add_vector_store`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorDb {
    /// Pinecone.
    PineconeDb,
    /// Weaviate.
    WeaviateDb,
    /// Postgres (`pgvector`).
    PostgresDb,
    /// MongoDB (vector search).
    MongoDb,
}

impl VectorDb {
    /// The exact wire string the server's integration-provider registry expects, matching
    /// python's `VectorDB` enum values.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PineconeDb => "pineconedb",
            Self::WeaviateDb => "weaviatedb",
            Self::PostgresDb => "pgvectordb",
            Self::MongoDb => "mongovectordb",
        }
    }
}

/// A provider-specific `configuration` payload for an AI model or vector-DB integration.
///
/// The Conductor server stores this as an opaque `Map<String, Object>` -- there is no single
/// canonical schema, since each integration provider plugin reads whatever keys it expects
/// (confirmed against `docs/agents/` and the real server's `ai/VECTORDB_CONFIGURATION.md`, which
/// describes a *different*, static YAML-configured set of vector-DB instances, not this dynamic
/// per-provider map). Implementations here match python's exactly, including its inconsistent
/// key casing (`"api_key"` vs. `"projectName"` on the same [`PineconeConfig`]) -- that isn't a
/// bug to "fix," it's whatever each real provider integration already reads server-side.
pub trait IntegrationConfig {
    /// Build the `configuration` map to send as part of an `IntegrationUpdate`/
    /// `IntegrationApiUpdate`.
    fn to_config(&self) -> HashMap<String, Value>;
}

/// Configuration for a Weaviate vector-DB integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeaviateConfig {
    /// API key.
    pub api_key: String,
    /// Weaviate endpoint URL.
    pub endpoint: String,
    /// Weaviate class name. Stored for callers who need it, but -- matching python's
    /// `WeaviateConfig.to_dict()` exactly -- **not** included in [`IntegrationConfig::to_config`].
    pub classname: String,
}

impl WeaviateConfig {
    /// Create a new Weaviate configuration.
    #[must_use]
    pub fn new(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        classname: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            classname: classname.into(),
        }
    }
}

impl IntegrationConfig for WeaviateConfig {
    fn to_config(&self) -> HashMap<String, Value> {
        HashMap::from([
            ("api_key".to_owned(), Value::String(self.api_key.clone())),
            ("endpoint".to_owned(), Value::String(self.endpoint.clone())),
        ])
    }
}

/// Configuration for an `OpenAI` integration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenAiConfig {
    /// API key.
    pub api_key: Option<String>,
}

impl OpenAiConfig {
    /// Create a new `OpenAI` configuration with an explicit API key.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: Some(api_key.into()),
        }
    }

    /// Create a new `OpenAI` configuration, reading the API key from the `OPENAI_API_KEY`
    /// environment variable (matching python's `OpenAIConfig(api_key=None)` fallback).
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").ok(),
        }
    }
}

impl IntegrationConfig for OpenAiConfig {
    fn to_config(&self) -> HashMap<String, Value> {
        HashMap::from([(
            "api_key".to_owned(),
            self.api_key.clone().map_or(Value::Null, Value::String),
        )])
    }
}

/// Configuration for an Azure `OpenAI` integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureOpenAiConfig {
    /// API key.
    pub api_key: String,
    /// Azure `OpenAI` endpoint URL.
    pub endpoint: String,
}

impl AzureOpenAiConfig {
    /// Create a new Azure `OpenAI` configuration.
    #[must_use]
    pub fn new(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: endpoint.into(),
        }
    }
}

impl IntegrationConfig for AzureOpenAiConfig {
    fn to_config(&self) -> HashMap<String, Value> {
        HashMap::from([
            ("api_key".to_owned(), Value::String(self.api_key.clone())),
            ("endpoint".to_owned(), Value::String(self.endpoint.clone())),
        ])
    }
}

/// Configuration for a Pinecone vector-DB integration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PineconeConfig {
    /// API key.
    pub api_key: Option<String>,
    /// Pinecone endpoint URL.
    pub endpoint: Option<String>,
    /// Pinecone environment (e.g. `"us-west1-gcp"`).
    pub environment: Option<String>,
    /// Pinecone project name.
    pub project_name: Option<String>,
}

impl PineconeConfig {
    /// Create a new Pinecone configuration with explicit values.
    #[must_use]
    pub fn new(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        environment: impl Into<String>,
        project_name: impl Into<String>,
    ) -> Self {
        Self {
            api_key: Some(api_key.into()),
            endpoint: Some(endpoint.into()),
            environment: Some(environment.into()),
            project_name: Some(project_name.into()),
        }
    }

    /// Create a new Pinecone configuration, reading any unset field from
    /// `PINECONE_API_KEY`/`PINECONE_ENDPOINT`/`PINECONE_ENV`/`PINECONE_PROJECT` (matching
    /// python's `PineconeConfig(...)` per-field fallback).
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("PINECONE_API_KEY").ok(),
            endpoint: std::env::var("PINECONE_ENDPOINT").ok(),
            environment: std::env::var("PINECONE_ENV").ok(),
            project_name: std::env::var("PINECONE_PROJECT").ok(),
        }
    }
}

impl IntegrationConfig for PineconeConfig {
    fn to_config(&self) -> HashMap<String, Value> {
        // `projectName` is camelCase while every other key here is snake_case -- ported exactly
        // from python's `PineconeConfig.to_dict()`; see this module's doc comment for why.
        HashMap::from([
            (
                "api_key".to_owned(),
                self.api_key.clone().map_or(Value::Null, Value::String),
            ),
            (
                "endpoint".to_owned(),
                self.endpoint.clone().map_or(Value::Null, Value::String),
            ),
            (
                "projectName".to_owned(),
                self.project_name.clone().map_or(Value::Null, Value::String),
            ),
            (
                "environment".to_owned(),
                self.environment.clone().map_or(Value::Null, Value::String),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_provider_wire_strings() {
        assert_eq!(LlmProvider::AzureOpenAi.as_str(), "azure_openai");
        assert_eq!(LlmProvider::OpenAi.as_str(), "openai");
        assert_eq!(LlmProvider::Grok.as_str(), "Grok");
    }

    #[test]
    fn test_vector_db_wire_strings() {
        assert_eq!(VectorDb::PineconeDb.as_str(), "pineconedb");
        assert_eq!(VectorDb::MongoDb.as_str(), "mongovectordb");
    }

    #[test]
    fn test_weaviate_config_excludes_classname() {
        let config = WeaviateConfig::new("key", "https://weaviate.example", "MyClass");
        let map = config.to_config();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("api_key"), Some(&Value::String("key".to_owned())));
        assert_eq!(
            map.get("endpoint"),
            Some(&Value::String("https://weaviate.example".to_owned()))
        );
        assert!(!map.contains_key("classname"));
    }

    #[test]
    fn test_pinecone_config_uses_project_name_camel_case() {
        let config = PineconeConfig::new("key", "endpoint", "us-west1-gcp", "my-project");
        let map = config.to_config();
        assert_eq!(
            map.get("projectName"),
            Some(&Value::String("my-project".to_owned()))
        );
        assert!(!map.contains_key("project_name"));
    }

    #[test]
    fn test_openai_config_new_sets_api_key() {
        let config = OpenAiConfig::new("sk-test");
        assert_eq!(
            config.to_config().get("api_key"),
            Some(&Value::String("sk-test".to_owned()))
        );
    }
}
