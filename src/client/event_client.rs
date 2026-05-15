// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use serde::{Deserialize, Serialize};

/// Client for event operations (queue configurations)
#[derive(Clone)]
pub struct EventClient {
    api: ApiClient,
}

/// Queue configuration for event handling
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueConfiguration {
    /// Queue name (e.g., topic name for Kafka)
    pub queue_name: String,

    /// Queue type (e.g., "kafka", "sqs", "amqp")
    pub queue_type: String,

    /// Queue-specific configuration
    #[serde(default)]
    pub configuration: serde_json::Value,
}

impl QueueConfiguration {
    /// Create a new queue configuration
    pub fn new(queue_type: impl Into<String>, queue_name: impl Into<String>) -> Self {
        Self {
            queue_name: queue_name.into(),
            queue_type: queue_type.into(),
            configuration: serde_json::Value::Null,
        }
    }

    /// Create a Kafka queue configuration
    pub fn kafka(topic: impl Into<String>) -> Self {
        Self::new("kafka", topic)
    }

    /// Create an SQS queue configuration
    pub fn sqs(queue_url: impl Into<String>) -> Self {
        Self::new("sqs", queue_url)
    }

    /// Create an AMQP queue configuration
    pub fn amqp(queue_name: impl Into<String>) -> Self {
        Self::new("amqp_queue", queue_name)
    }

    /// Set the configuration
    pub fn with_configuration(mut self, config: serde_json::Value) -> Self {
        self.configuration = config;
        self
    }
}

impl EventClient {
    /// Create a new event client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Delete a queue configuration
    pub async fn delete_queue_configuration(
        &self,
        queue_config: &QueueConfiguration,
    ) -> Result<()> {
        let path = format!(
            "/event/queue/config/{}/{}",
            queue_config.queue_type, queue_config.queue_name
        );
        self.api.delete_no_content(ApiPath::templated(&path, "/event/queue/config/{queueType}/{queueName}")).await
    }

    /// Get a Kafka queue configuration by topic
    pub async fn get_kafka_queue_configuration(
        &self,
        queue_topic: &str,
    ) -> Result<QueueConfiguration> {
        self.get_queue_configuration("kafka", queue_topic).await
    }

    /// Get a queue configuration by type and name
    pub async fn get_queue_configuration(
        &self,
        queue_type: &str,
        queue_name: &str,
    ) -> Result<QueueConfiguration> {
        let path = format!("/event/queue/config/{}/{}", queue_type, queue_name);
        self.api.get(ApiPath::templated(&path, "/event/queue/config/{queueType}/{queueName}")).await
    }

    /// Create or update a queue configuration
    pub async fn put_queue_configuration(&self, queue_config: &QueueConfiguration) -> Result<()> {
        let path = format!(
            "/event/queue/config/{}/{}",
            queue_config.queue_type, queue_config.queue_name
        );
        self.api
            .put_no_response(ApiPath::templated(&path, "/event/queue/config/{queueType}/{queueName}"), &queue_config.configuration)
            .await
    }

    /// Get all queue configurations
    pub async fn get_all_queue_configurations(&self) -> Result<Vec<QueueConfiguration>> {
        self.api.get("/event/queue/config").await
    }

    /// Get event handlers for a specific event
    pub async fn get_event_handlers(
        &self,
        event: &str,
        active_only: bool,
    ) -> Result<Vec<serde_json::Value>> {
        let path = format!(
            "/event/{}?activeOnly={}",
            urlencoding::encode(event),
            active_only
        );
        self.api.get(ApiPath::templated(&path, "/event/{event}")).await
    }

    /// Get all event handlers
    pub async fn get_all_event_handlers(&self) -> Result<Vec<serde_json::Value>> {
        self.api.get("/event").await
    }

    /// Register an event handler
    pub async fn register_event_handler(&self, event_handler: &serde_json::Value) -> Result<()> {
        self.api.post_no_response("/event", event_handler).await
    }

    /// Update an event handler
    pub async fn update_event_handler(&self, event_handler: &serde_json::Value) -> Result<()> {
        self.api.put_no_response("/event", event_handler).await
    }

    /// Remove an event handler
    pub async fn remove_event_handler(&self, name: &str) -> Result<()> {
        let path = format!("/event/{}", name);
        self.api.delete_no_content(ApiPath::templated(&path, "/event/{event}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_event_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = EventClient::new(api);
    }

    #[test]
    fn test_queue_configuration() {
        let kafka_config = QueueConfiguration::kafka("my-topic");
        assert_eq!(kafka_config.queue_type, "kafka");
        assert_eq!(kafka_config.queue_name, "my-topic");

        let sqs_config =
            QueueConfiguration::sqs("https://sqs.us-east-1.amazonaws.com/123/my-queue");
        assert_eq!(sqs_config.queue_type, "sqs");
    }
}
