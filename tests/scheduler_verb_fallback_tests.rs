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

    // `put_no_body` exists precisely so these endpoints -- mapped as a bare
    // @PutMapping with no @RequestBody -- get no payload and no Content-Type.
    // The generic `put` helper would send a JSON body instead, so pin it here;
    // nothing else in the suite would notice the difference.
    let requests = mock_server
        .received_requests()
        .await
        .expect("request recording is on by default");
    assert_eq!(
        requests.len(),
        1,
        "expected exactly one request, got {requests:?}"
    );
    let put = &requests[0];
    assert!(
        put.body.is_empty(),
        "expected an empty PUT body, got {:?}",
        String::from_utf8_lossy(&put.body)
    );
    assert!(
        !put.headers
            .keys()
            .any(|k| k.to_string().eq_ignore_ascii_case("content-type")),
        "expected no Content-Type on a bodyless PUT, got {:?}",
        put.headers
            .keys()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
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

    Mock::given(method("PUT"))
        .and(path("/api/scheduler/schedules/missing/pause"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&mock_server)
        .await;

    // The GET mock exists only to catch a fallback that shouldn't happen.
    // Leaving it unmounted would not work: wiremock answers an unmatched
    // request with a 404 and verifies only the expectations of the mocks that
    // *are* mounted, so a stray GET would be invisible here and the call would
    // still end in an `Err` -- the test would pass either way. Mounted with
    // `expect(0)`, the stray request lands on a mock whose hit count is
    // checked, and verification fails when `MockServer` is dropped.
    Mock::given(method("GET"))
        .and(path("/api/scheduler/schedules/missing/pause"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
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

    // No PUT mock registered at all. A PUT would go unmatched -- which on its
    // own proves nothing, since wiremock ignores unmatched requests -- but it
    // would also leave this GET mock's `expect(1)` unmet, and that does fail
    // verification when the server is dropped.
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
