// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.
//
// Verb contract for the scheduler client, verified against a local mock server
// so it runs with no Conductor instance and on both CI jobs.
//
// - Per-schedule pause/resume (`/scheduler/schedules/{name}/pause|resume`) are
//   `PUT`-only on OSS Conductor (scheduler/core/.../rest/SchedulerResource.java
//   maps just `@PutMapping`). Orkes Conductor accepts both GET and PUT via
//   `@RequestMapping(method = {GET, PUT})`, added in 2026-07; deployments older
//   than that are GET-only. Hence: PUT first, fall back to GET on a 405 -- and
//   only on a 405.
// - Admin/bulk endpoints (`/scheduler/admin/pause|resume|requeue`) are
//   `@GetMapping` on both server families -- no fallback, and no PUT should
//   ever be sent.

use conductor::http::ApiClient;
use conductor::{ConductorClient, Configuration};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(mock_server: &MockServer) -> ConductorClient {
    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create api client");
    ConductorClient::from_api_client(api)
}

#[tokio::test]
async fn test_pause_schedule_tries_put_first() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/api/scheduler/schedules/sched-1/pause"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().pause_schedule("sched-1").await;

    assert!(
        result.is_ok(),
        "expected PUT to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_pause_schedule_falls_back_to_get_on_405() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/api/scheduler/schedules/sched-1/pause"))
        .respond_with(ResponseTemplate::new(405))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/scheduler/schedules/sched-1/pause"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().pause_schedule("sched-1").await;

    assert!(
        result.is_ok(),
        "expected GET fallback to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_pause_schedule_does_not_fall_back_on_non_405_error() {
    let mock_server = MockServer::start().await;

    // Only a PUT mock is registered; if the client incorrectly fell back to
    // GET here, the unmatched request would cause wiremock to panic on drop,
    // failing the test.
    Mock::given(method("PUT"))
        .and(path("/api/scheduler/schedules/missing/pause"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().pause_schedule("missing").await;

    assert!(result.is_err(), "expected the 404 to propagate");
}

#[tokio::test]
async fn test_resume_schedule_falls_back_to_get_on_405() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/api/scheduler/schedules/sched-1/resume"))
        .respond_with(ResponseTemplate::new(405))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/scheduler/schedules/sched-1/resume"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().resume_schedule("sched-1").await;

    assert!(
        result.is_ok(),
        "expected GET fallback to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_pause_all_schedules_only_sends_get() {
    let mock_server = MockServer::start().await;

    // No PUT mock registered at all: if the client sent PUT instead of GET,
    // the request would go unmatched and this assertion would fail.
    Mock::given(method("GET"))
        .and(path("/api/scheduler/admin/pause"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().pause_all_schedules().await;

    assert!(
        result.is_ok(),
        "expected GET to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_resume_all_schedules_only_sends_get() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/scheduler/admin/resume"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client.scheduler_client().resume_all_schedules().await;

    assert!(
        result.is_ok(),
        "expected GET to succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_requeue_all_execution_records_only_sends_get() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/scheduler/admin/requeue"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let result = client
        .scheduler_client()
        .requeue_all_execution_records()
        .await;

    assert!(
        result.is_ok(),
        "expected GET to succeed: {:?}",
        result.err()
    );
}
