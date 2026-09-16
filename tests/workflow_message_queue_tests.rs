// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Wiremock-based tests for the Workflow Message Queue (WMQ) API surface -- unlike the rest of
//! `workflow_client_tests.rs`, these don't need a live server with
//! `conductor.workflow-message-queue.enabled=true`, just the right request/response shape.

use conductor::client::WorkflowClient;
use conductor::http::ApiClient;
use conductor::Configuration;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_send_message_posts_payload_and_returns_message_id() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/workflow/wf-123/messages"))
        .and(body_json(
            serde_json::json!({"event": "payment_confirmed", "amount": 99.99}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("f3c2a1b0-1234-5678-9abc-def012345678")
                .insert_header("content-type", "text/plain"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = WorkflowClient::new(api);

    let message_id = client
        .send_message(
            "wf-123",
            &serde_json::json!({"event": "payment_confirmed", "amount": 99.99}),
        )
        .await
        .expect("send_message request failed");

    assert_eq!(message_id, "f3c2a1b0-1234-5678-9abc-def012345678");
}

#[tokio::test]
async fn test_send_message_surfaces_workflow_not_running_as_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/workflow/wf-not-running/messages"))
        .respond_with(
            ResponseTemplate::new(409).set_body_string("Workflow wf-not-running is not RUNNING"),
        )
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = WorkflowClient::new(api);

    let result = client
        .send_message("wf-not-running", &serde_json::json!({"ping": true}))
        .await;

    result.unwrap_err();
}
