// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Wiremock-based tests for `AgentHandle::join_with_options`'s stall detection -- see
//! `src/agents/liveness.rs` for the design notes.

#![cfg(feature = "agents")]

use conductor::agents::{AgentHandle, StallPolicy};
use conductor::client::AgentClient;
use conductor::http::ApiClient;
use conductor::Configuration;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn running_status() -> serde_json::Value {
    serde_json::json!({"status": "RUNNING", "isRunning": true})
}

fn completed_status() -> serde_json::Value {
    serde_json::json!({
        "status": "COMPLETED",
        "isComplete": true,
        "output": {"result": "ok"},
    })
}

fn workflow_with_stalled_task(seconds_queued: i64) -> serde_json::Value {
    let scheduled_time = chrono::Utc::now().timestamp_millis() - seconds_queued * 1000;
    serde_json::json!({
        "workflowId": "exec-1",
        "status": "RUNNING",
        "tasks": [{
            "taskId": "t1",
            "taskDefName": "my_stuck_tool",
            "status": "SCHEDULED",
            "pollCount": 0,
            "scheduledTime": scheduled_time,
        }],
    })
}

#[tokio::test]
async fn test_join_with_options_raises_on_a_detected_stall() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/agent/exec-1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(running_status()))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/workflow/exec-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(workflow_with_stalled_task(45)))
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = AgentClient::new(api);
    let handle = AgentHandle::new(client, "exec-1");

    let result = handle
        .join_with_options(30.0, Duration::from_millis(1), StallPolicy::Raise)
        .await;

    let err = result.expect_err("expected a WorkerStall error");
    assert!(
        err.to_string().contains("Worker stall detected"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_join_with_options_warn_policy_keeps_waiting_until_completion() {
    let mock_server = MockServer::start().await;

    // First status poll: RUNNING (with a stall present); second: COMPLETED.
    Mock::given(method("GET"))
        .and(path("/api/agent/exec-2/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(running_status()))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/agent/exec-2/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completed_status()))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/workflow/exec-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(workflow_with_stalled_task(45)))
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = AgentClient::new(api);
    let handle = AgentHandle::new(client, "exec-2");

    let result = handle
        .join_with_options(30.0, Duration::from_millis(1), StallPolicy::Warn)
        .await
        .expect("join should complete despite the stall under StallPolicy::Warn");

    assert_eq!(result.output, serde_json::json!({"result": "ok"}));
}

#[tokio::test]
async fn test_join_with_options_ignores_a_task_below_the_stall_threshold() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/agent/exec-3/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(running_status()))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/agent/exec-3/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completed_status()))
        .mount(&mock_server)
        .await;

    // Queued only 2s -- well under the 30s threshold.
    Mock::given(method("GET"))
        .and(path("/api/workflow/exec-3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(workflow_with_stalled_task(2)))
        .mount(&mock_server)
        .await;

    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    let client = AgentClient::new(api);
    let handle = AgentHandle::new(client, "exec-3");

    let result = handle
        .join_with_options(30.0, Duration::from_millis(1), StallPolicy::Raise)
        .await
        .expect("no stall should be reported below the threshold");

    assert_eq!(result.output, serde_json::json!({"result": "ok"}));
}
