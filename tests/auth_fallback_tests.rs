use conductor::http::ApiClient;
use conductor::Configuration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_auth_fallback_on_404() {
    // 1. Start Mock Server
    let mock_server = MockServer::start().await;

    // 2. Mock 404 for token endpoint (simulating OSS Conductor or missing auth)
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1) // Should be called once
        .mount(&mock_server)
        .await;

    // 3. Mock 200 for a business endpoint (e.g., list tasks or health) to verify the call proceeds
    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json("OK"))
        .mount(&mock_server)
        .await;

    // 4. Configure client with Auth
    let server_url = format!("{}/api", mock_server.uri());
    let config = Configuration::new(&server_url).with_auth("dummy_key", "dummy_secret");

    let client = ApiClient::new(config).expect("Failed to create client");

    // 5. Trigger a request request
    // calling a simple endpoint that would require auth if enabled
    let response: Result<String, _> = client.get("/health").await;

    // 6. Assertions
    // The request should succeed because the client should have disabled auth after the 404
    assert!(response.is_ok(), "Request failed: {:?}", response.err());
    assert_eq!(response.unwrap(), "OK");
}
