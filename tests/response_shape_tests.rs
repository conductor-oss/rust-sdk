// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.
//
// Response-shape contracts that the live integration suites can't pin down,
// verified against a local mock server so they run with no Conductor instance.
//
// The live tests for these only ever observe empty/degenerate responses -- the
// permissions endpoint returns an empty list for a freshly created user, and
// Orkes' getQueueNames() is a hardcoded Map.of() -- so a wrong shape would
// deserialize cleanly and pass. These use realistic payloads instead.

use conductor::http::ApiClient;
use conductor::models::{AccessType, TargetType};
use conductor::{ConductorClient, Configuration};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(mock_server: &MockServer) -> ConductorClient {
    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create api client");
    ConductorClient::from_api_client(api)
}

/// `GET /users/{id}/permissions` answers with an envelope object, not a bare
/// array: Orkes `rest/model/responses/GrantedAccessResponse` is
/// `{grantedAccess: [{target, access, tag}]}`.
#[tokio::test]
async fn test_granted_permissions_for_user_unwraps_envelope() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/users/alice/permissions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "grantedAccess": [
                {
                    "target": {"type": "WORKFLOW_DEF", "id": "order_flow"},
                    "access": ["READ", "EXECUTE"],
                    "tag": "team:payments"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let perms = client
        .authorization_client()
        .get_granted_permissions_for_user("alice")
        .await
        .expect("get_granted_permissions_for_user should succeed");

    assert_eq!(
        perms.len(),
        1,
        "envelope should unwrap to one grant: {perms:?}"
    );
    let target = perms[0].target.as_ref().expect("target should be present");
    assert_eq!(target.id, "order_flow");
    assert!(matches!(target.target_type, TargetType::WorkflowDef));
    assert_eq!(perms[0].access, vec![AccessType::Read, AccessType::Execute]);
    assert_eq!(perms[0].tag.as_deref(), Some("team:payments"));
}

/// The group endpoint shares the same envelope.
#[tokio::test]
async fn test_granted_permissions_for_group_unwraps_envelope() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/groups/payments/permissions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "grantedAccess": [
                {"target": {"type": "TASK_DEF", "id": "charge_card"}, "access": ["UPDATE"]}
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let perms = client
        .authorization_client()
        .get_granted_permissions_for_group("payments")
        .await
        .expect("get_granted_permissions_for_group should succeed");

    assert_eq!(
        perms.len(),
        1,
        "envelope should unwrap to one grant: {perms:?}"
    );
    assert_eq!(perms[0].access, vec![AccessType::Update]);
    assert_eq!(perms[0].tag, None);
}

/// `POST /applications/{id}/accessKeys` answers `{id, secret}` -- no `status`,
/// unlike the `AccessKeyResponse` the list/toggle endpoints return.
#[tokio::test]
async fn test_create_access_key_response_has_no_status() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/applications/app-1/accessKeys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "key-1",
            "secret": "sh-sec-abc"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let key = client
        .authorization_client()
        .create_access_key("app-1")
        .await
        .expect("create_access_key should succeed");

    assert_eq!(key.id, "key-1");
    assert_eq!(key.secret, "sh-sec-abc");
}

/// `GET /event/queue/config` answers a `{queueIdentifier: value}` object --
/// Orkes declares it as `Map<String, String> getQueueNames()`.
#[tokio::test]
async fn test_get_all_queue_configurations_is_a_string_map() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/event/queue/config"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sqs:my-queue": "my-queue",
            "kafka:my-topic": "my-topic"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let configs = client
        .event_client()
        .get_all_queue_configurations()
        .await
        .expect("get_all_queue_configurations should succeed");

    assert_eq!(
        configs.get("sqs:my-queue").map(String::as_str),
        Some("my-queue")
    );
    assert_eq!(
        configs.get("kafka:my-topic").map(String::as_str),
        Some("my-topic")
    );
}

/// `GET /scheduler/schedules/{name}` nests the workflow name inside
/// `startWorkflowRequest`; there is no top-level `workflowName` on either
/// server family.
#[tokio::test]
async fn test_workflow_schedule_reads_workflow_name_from_start_request() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/scheduler/schedules/nightly"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "nightly",
            "cronExpression": "0 0 0 * * ?",
            "paused": false,
            "runCatchupScheduleInstances": false,
            "zoneId": "UTC",
            "updatedTime": 1_700_000_000_000i64,
            "startWorkflowRequest": {"name": "order_flow", "version": 3}
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let schedule = client
        .scheduler_client()
        .get_schedule("nightly")
        .await
        .expect("get_schedule should succeed");

    assert_eq!(schedule.name, "nightly");
    let start = schedule
        .start_workflow_request
        .as_ref()
        .expect("startWorkflowRequest should be present");
    assert_eq!(start.name, "order_flow");
    assert_eq!(start.version, Some(3));
    // `updatedTime`, not `updateTime` -- the field name the server actually sends.
    assert_eq!(schedule.updated_time, Some(1_700_000_000_000));
}
