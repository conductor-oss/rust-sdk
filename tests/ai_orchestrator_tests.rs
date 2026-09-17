// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::client::AiOrchestrator;
use conductor::http::ApiClient;
use conductor::models::OpenAiConfig;
use conductor::Configuration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(mock_server: &MockServer) -> AiOrchestrator {
    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    AiOrchestrator::from_api_client(api)
}

#[tokio::test]
async fn test_add_ai_integration_creates_missing_integration_and_model() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/integrations/provider/my-openai"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/api/integrations/provider/my-openai"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path(
            "/api/integrations/provider/my-openai/integration/gpt-4o",
        ))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path(
            "/api/integrations/provider/my-openai/integration/gpt-4o",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let orchestrator = client_for(&mock_server);
    let config = OpenAiConfig::new("sk-test");

    orchestrator
        .add_ai_integration(
            "my-openai",
            conductor::models::LlmProvider::OpenAi,
            &["gpt-4o".to_owned()],
            "test integration",
            &config,
            false,
        )
        .await
        .expect("add_ai_integration failed");
}

#[tokio::test]
async fn test_add_ai_integration_skips_existing_integration_without_overwrite() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/integrations/provider/my-openai"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "my-openai",
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path(
            "/api/integrations/provider/my-openai/integration/gpt-4o",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "gpt-4o",
            "integrationName": "my-openai",
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // No PUT mocks registered at all: if `add_ai_integration` tried to save either the
    // integration or the model despite both already existing, wiremock would panic on an
    // unmatched request.
    let orchestrator = client_for(&mock_server);
    let config = OpenAiConfig::new("sk-test");

    orchestrator
        .add_ai_integration(
            "my-openai",
            conductor::models::LlmProvider::OpenAi,
            &["gpt-4o".to_owned()],
            "test integration",
            &config,
            false,
        )
        .await
        .expect("add_ai_integration failed");
}

#[tokio::test]
async fn test_get_prompt_template_returns_none_when_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/prompts/missing"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    let orchestrator = client_for(&mock_server);
    let result = orchestrator
        .get_prompt_template("missing")
        .await
        .expect("get_prompt_template returned an error instead of None");

    assert!(result.is_none());
}
