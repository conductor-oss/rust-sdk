// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::client::AgentClient;
use conductor::http::ApiClient;
use conductor::Configuration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_compile_agent_posts_to_agent_compile() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/agent/compile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = AgentClient::new(api);

    let response = client
        .compile_agent(&serde_json::json!({"name": "coordinator"}))
        .await
        .expect("compile_agent request failed");

    assert_eq!(response, serde_json::json!({"ok": true}));
}

#[tokio::test]
async fn test_stop_posts_to_agent_id_stop() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/agent/exec-123/stop"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = AgentClient::new(api);

    client.stop("exec-123").await.expect("stop request failed");
}
