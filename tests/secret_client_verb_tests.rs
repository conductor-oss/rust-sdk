// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.
//
// Verb and content-type contract for the secret client, verified against a local
// mock server so it runs with no Conductor instance and on both CI jobs.
//
// Both server families split the two verbs on `/secrets`:
//   - POST /secrets -> list ALL secret names (OSS SecretController.listAllNames,
//     Orkes SecretResource.listAllSecretNames)
//   - GET  /secrets -> list only the names the caller has access to
// On OSS the two agree because there is no RBAC, so calling the wrong one there
// looks correct; against Orkes it silently returns an access-filtered subset.
//
// `GET /secrets/{key}` is declared `produces = MediaType.TEXT_PLAIN_VALUE` on
// both, so the value must be read as raw text, not parsed as a JSON string.

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
async fn test_list_all_secret_names_uses_post() {
    let mock_server = MockServer::start().await;

    // Only POST is mocked. A GET would go unmatched and fail the call, which is
    // exactly the regression this guards: GET is the access-filtered listing.
    Mock::given(method("POST"))
        .and(path("/api/secrets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec!["ALPHA", "BETA"]))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let names = client
        .secret_client()
        .list_all_secret_names()
        .await
        .expect("list_all_secret_names should succeed");

    assert!(names.contains("ALPHA"), "got {names:?}");
    assert!(names.contains("BETA"), "got {names:?}");
}

#[tokio::test]
async fn test_list_grantable_secrets_uses_get() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/secrets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec!["ALPHA"]))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let names = client
        .secret_client()
        .list_secrets_that_user_can_grant_access_to()
        .await
        .expect("list_secrets_that_user_can_grant_access_to should succeed");

    assert_eq!(names, vec!["ALPHA".to_string()]);
}

#[tokio::test]
async fn test_get_secret_reads_text_plain_verbatim() {
    let mock_server = MockServer::start().await;

    // A raw text/plain body, as both servers send. Parsing this as JSON would
    // fail outright; parsing a quoted JSON string instead would silently strip
    // characters from values that happen to look like JSON.
    Mock::given(method("GET"))
        .and(path("/api/secrets/MY_KEY"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("s3cr3t-\"value\"-{not json}", "text/plain"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let value = client
        .secret_client()
        .get_secret("MY_KEY")
        .await
        .expect("get_secret should succeed");

    assert_eq!(value, "s3cr3t-\"value\"-{not json}");
}
