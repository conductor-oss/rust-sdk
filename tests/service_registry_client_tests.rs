// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::client::ServiceRegistryClient;
use conductor::http::ApiClient;
use conductor::models::{ServiceMethod, ServiceRegistry};
use conductor::Configuration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client_for(mock_server: &MockServer) -> ServiceRegistryClient {
    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url);
    let api = ApiClient::new(config).expect("failed to create client");
    ServiceRegistryClient::new(api)
}

#[tokio::test]
async fn test_get_registered_services_gets_registry_service() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/registry/service"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "orders-service", "type": "HTTP", "serviceURI": "http://orders:8080"}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let services = client
        .get_registered_services()
        .await
        .expect("get_registered_services failed");

    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, Some("orders-service".to_owned()));
}

#[tokio::test]
async fn test_add_or_update_service_posts_registry_service() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/registry/service"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let registry = ServiceRegistry::http("orders-service", "http://orders:8080");
    client
        .add_or_update_service(&registry)
        .await
        .expect("add_or_update_service failed");
}

#[tokio::test]
async fn test_remove_service_deletes_by_name() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/api/registry/service/orders-service"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    client
        .remove_service("orders-service")
        .await
        .expect("remove_service failed");
}

#[tokio::test]
async fn test_open_circuit_breaker_returns_transition_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/api/registry/service/orders-service/circuit-breaker/open",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "service": "orders-service",
            "previousState": "CLOSED",
            "currentState": "OPEN"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let response = client
        .open_circuit_breaker("orders-service")
        .await
        .expect("open_circuit_breaker failed");

    assert_eq!(response.current_state, Some("OPEN".to_owned()));
    assert!(response.is_open());
}

#[tokio::test]
async fn test_is_circuit_breaker_open_reads_status() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/api/registry/service/orders-service/circuit-breaker/status",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "currentState": "closed"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let is_open = client
        .is_circuit_breaker_open("orders-service")
        .await
        .expect("is_circuit_breaker_open failed");

    assert!(!is_open);
}

#[tokio::test]
async fn test_add_or_update_method_posts_to_registry_methods() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/registry/service/orders-service/methods"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let service_method = ServiceMethod {
        operation_name: Some("getOrder".to_owned()),
        method_name: Some("GetOrder".to_owned()),
        method_type: Some("UNARY".to_owned()),
        ..ServiceMethod::default()
    };
    client
        .add_or_update_method("orders-service", &service_method)
        .await
        .expect("add_or_update_method failed");
}

#[tokio::test]
async fn test_remove_method_sends_query_params() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/api/registry/service/orders-service/methods"))
        .and(query_param("serviceName", "orders-service"))
        .and(query_param("method", "GetOrder"))
        .and(query_param("methodType", "UNARY"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    client
        .remove_method("orders-service", "orders-service", "GetOrder", "UNARY")
        .await
        .expect("remove_method failed");
}

#[tokio::test]
async fn test_proto_data_round_trip_uses_raw_bytes() {
    let mock_server = MockServer::start().await;
    let proto_bytes = b"syntax = \"proto3\";".to_vec();

    Mock::given(method("POST"))
        .and(path(
            "/api/registry/service/protos/orders-service/orders.proto",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path(
            "/api/registry/service/protos/orders-service/orders.proto",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(proto_bytes.clone()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    client
        .set_proto_data("orders-service", "orders.proto", proto_bytes.clone())
        .await
        .expect("set_proto_data failed");

    let fetched = client
        .get_proto_data("orders-service", "orders.proto")
        .await
        .expect("get_proto_data failed");

    assert_eq!(fetched, proto_bytes);
}

#[tokio::test]
async fn test_discover_with_create_sends_create_query_param() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/registry/service/orders-service/discover"))
        .and(query_param("create", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"operationName": "getOrder", "methodType": "UNARY"}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = client_for(&mock_server);
    let methods = client
        .discover("orders-service", true)
        .await
        .expect("discover failed");

    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0].operation_name, Some("getOrder".to_owned()));
}
