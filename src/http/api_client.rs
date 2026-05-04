// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use reqwest::{Client, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::configuration::Configuration;
use crate::error::{ConductorError, Result};
use crate::http::metrics::{HttpMetricsObserver, NoopHttpMetricsObserver};

/// Token response from authentication endpoint
#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    token: String,
}

/// Maximum consecutive auth failures before stopping retry attempts
const MAX_AUTH_FAILURES: u32 = 5;

/// HTTP API client for Conductor server
///
/// Thread-safe and cloneable. Multiple clones share the same connection pool
/// and authentication state.
///
/// # Authentication
///
/// The client automatically handles authentication when credentials are configured:
/// - Fetches tokens from `/token` endpoint using key/secret
/// - Proactively refreshes tokens before TTL expiration (default: 45 min)
/// - Automatically retries requests on 401 after refreshing token
/// - Gracefully handles OSS Conductor (no auth endpoint returns 404)
///
/// # Environment Variables
///
/// Configure via environment variables or `.env` file:
/// - `CONDUCTOR_SERVER_URL`: Server API URL (default: http://localhost:8080/api)
/// - `CONDUCTOR_AUTH_KEY`: Authentication key ID (Orkes Conductor)
/// - `CONDUCTOR_AUTH_SECRET`: Authentication secret (Orkes Conductor)
/// - `CONDUCTOR_AUTH_TOKEN_TTL_MINS`: Token TTL in minutes (default: 45)
/// - `CONDUCTOR_DEBUG`: Enable debug logging (true/false)
/// - `CONDUCTOR_TIMEOUT_SECS`: Request timeout in seconds (default: 30)
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    config: Arc<RwLock<Configuration>>,
    base_url: String,
    /// Path component of `base_url` (e.g. `"/api"`), prepended to endpoint
    /// paths when recording the `uri` metric label so the label matches the
    /// full request path as seen by all other SDKs.
    base_path: String,
    /// Track consecutive auth failures for backoff
    auth_failures: Arc<RwLock<u32>>,
    /// Last time we attempted token refresh (for backoff)
    last_refresh_attempt: Arc<RwLock<Option<Instant>>>,
    /// Mutex to ensure only one token refresh happens at a time
    token_refresh_lock: Arc<Mutex<()>>,
    /// Cached result of OSS detection (None = not yet probed)
    is_oss: Arc<RwLock<Option<bool>>>,
    /// Shared HTTP metrics observer. Starts as a no-op; can be replaced at
    /// runtime via [`ApiClient::set_http_metrics`] so that a
    /// `MetricsCollector` built later (e.g. inside `TaskHandler::enable_metrics`)
    /// can observe requests made by clients already vended from this
    /// `ApiClient` (since clones share the same inner `Arc`).
    http_metrics: Arc<parking_lot::RwLock<Arc<dyn HttpMetricsObserver>>>,
}

impl ApiClient {
    /// Create a new API client
    ///
    /// The client will automatically handle authentication if credentials are
    /// configured via `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET` environment
    /// variables, or via `Configuration::with_auth()`.
    pub fn new(config: Configuration) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(10)
            .build()?;

        let base_url = config.server_api_url.trim_end_matches('/').to_string();

        // Extract the path component of the server URL so it can be prepended
        // to endpoint paths in metric labels.
        // "http://host:8080/api" → "/api", "http://host:8080" → ""
        let base_path = base_url
            .find("://")
            .and_then(|scheme_end| {
                let after_scheme = scheme_end + 3;
                base_url[after_scheme..]
                    .find('/')
                    .map(|slash| after_scheme + slash)
            })
            .map(|abs_pos| base_url[abs_pos..].to_string())
            .unwrap_or_default();

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            base_url,
            base_path,
            auth_failures: Arc::new(RwLock::new(0)),
            last_refresh_attempt: Arc::new(RwLock::new(None)),
            token_refresh_lock: Arc::new(Mutex::new(())),
            is_oss: Arc::new(RwLock::new(None)),
            http_metrics: Arc::new(parking_lot::RwLock::new(NoopHttpMetricsObserver::arc())),
        })
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Install an [`HttpMetricsObserver`] that will be invoked after every
    /// request completes. Replaces the previously-installed observer.
    ///
    /// This swap is visible to every clone of this `ApiClient` (they share the
    /// same inner `Arc<RwLock<...>>`), so metrics can be enabled after service
    /// clients have already been vended.
    pub fn set_http_metrics(&self, observer: Arc<dyn HttpMetricsObserver>) {
        *self.http_metrics.write() = observer;
    }

    /// Snapshot the current observer. Internal helper so we drop the read
    /// lock before the request hot-path actually calls `observe`.
    #[inline]
    fn http_metrics(&self) -> Arc<dyn HttpMetricsObserver> {
        Arc::clone(&self.http_metrics.read())
    }

    /// Unified post-request bookkeeping: tracing log + observer callback.
    ///
    /// `path` should be the interpolated request path (no query string).
    /// `status_str` is the HTTP status code rendered as a string, or `"0"`
    /// for pre-response transport errors.
    #[inline]
    fn record_request(&self, method: &str, path: &str, status_str: &str, duration: Duration) {
        debug!(
            method = method,
            url = %format!("{}{}", self.base_url, path),
            status = status_str,
            duration_ms = %duration.as_millis(),
            "API request completed"
        );
        let uri_label = format!("{}{}", self.base_path, path);
        self.http_metrics()
            .observe(method, &uri_label, status_str, duration);
    }

    /// Convenience: call [`record_request`](Self::record_request) with a
    /// successful response's status code.
    #[inline]
    fn record_response(&self, method: &str, path: &str, status: StatusCode, duration: Duration) {
        self.record_request(method, path, status.as_str(), duration);
    }

    /// Send a prepared request, recording the outcome (success *and* transport
    /// failures) to the `http_metrics` observer and the `debug!` tracing log.
    ///
    /// Transport-level failures (no HTTP status produced) are observed with
    /// `status = "0"`, matching the convention of the canonical SDK metrics
    /// harmonization plan.
    async fn send_observed(
        &self,
        method: &str,
        path: &str,
        request: reqwest::RequestBuilder,
    ) -> Result<Response> {
        let start = Instant::now();
        let result = request.send().await;
        let duration = start.elapsed();
        match &result {
            Ok(resp) => self.record_response(method, path, resp.status(), duration),
            Err(_) => self.record_request(method, path, "0", duration),
        }
        result.map_err(ConductorError::Http)
    }

    /// GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request::<(), T>(reqwest::Method::GET, path, None)
            .await
    }

    /// GET request with query parameters
    pub async fn get_with_params<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.get(&url);
        request = self.add_auth_header(request).await?;
        request = request.query(params);

        let response = self.send_observed("GET", path, request).await?;
        self.handle_response(response).await
    }

    /// POST request
    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request_with_body(reqwest::Method::POST, path, body)
            .await
    }

    /// POST request returning raw text
    pub async fn post_text<B: Serialize>(&self, path: &str, body: &B) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;
        request = request.json(body);

        let response = self.send_observed("POST", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(response.text().await?)
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// PUT request
    pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request_with_body(reqwest::Method::PUT, path, body)
            .await
    }

    /// DELETE request
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request::<(), T>(reqwest::Method::DELETE, path, None)
            .await
    }

    /// DELETE request with no response body
    pub async fn delete_no_content(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.delete(&url);
        request = self.add_auth_header(request).await?;

        let response = self.send_observed("DELETE", path, request).await?;
        let status = response.status();

        if status.is_success() || status == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// DELETE request with body
    pub async fn delete_with_body<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.delete(&url);
        request = self.add_auth_header(request).await?;
        request = request.json(body);

        let response = self.send_observed("DELETE", path, request).await?;
        let status = response.status();

        if status.is_success() || status == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// DELETE request with query parameters
    pub async fn delete_with_params(&self, path: &str, params: &[(&str, &str)]) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.delete(&url);
        request = self.add_auth_header(request).await?;
        request = request.query(params);

        let response = self.send_observed("DELETE", path, request).await?;
        let status = response.status();

        if status.is_success() || status == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// POST request with no response
    pub async fn post_no_response<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;
        request = request.json(body);

        let response = self.send_observed("POST", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// POST request with no body
    pub async fn post_no_body<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;

        let response = self.send_observed("POST", path, request).await?;
        self.handle_response(response).await
    }

    /// POST request with no body and no response
    pub async fn post_no_body_no_response(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;

        let response = self.send_observed("POST", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// PUT request with no response
    pub async fn put_no_response<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.put(&url);
        request = self.add_auth_header(request).await?;
        request = request.json(body);

        let response = self.send_observed("PUT", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// PUT request with raw text body
    pub async fn put_raw(&self, path: &str, body: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.put(&url);
        request = self.add_auth_header(request).await?;
        request = request.body(body.to_string());
        request = request.header("Content-Type", "text/plain");

        let response = self.send_observed("PUT", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// POST request with JSON body and query parameters
    pub async fn post_with_params<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        params: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;
        request = request.query(params);
        request = request.json(body);

        let response = self.send_observed("POST", path, request).await?;
        self.handle_response(response).await
    }

    /// POST request with raw text body and query parameters
    pub async fn post_raw_with_params(
        &self,
        path: &str,
        body: &str,
        params: &[(&str, &str)],
    ) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.post(&url);
        request = self.add_auth_header(request).await?;
        request = request.query(params);
        request = request.body(body.to_string());
        request = request.header("Content-Type", "text/plain");

        let response = self.send_observed("POST", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// PUT request with raw text body and query parameters
    pub async fn put_raw_with_params(
        &self,
        path: &str,
        body: &str,
        params: &[(&str, &str)],
    ) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.put(&url);
        request = self.add_auth_header(request).await?;
        request = request.query(params);
        request = request.body(body.to_string());
        request = request.header("Content-Type", "text/plain");

        let response = self.send_observed("PUT", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// GET request with no response
    pub async fn get_no_response(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);

        let mut request = self.client.get(&url);
        request = self.add_auth_header(request).await?;

        let response = self.send_observed("GET", path, request).await?;
        let status = response.status();

        if status.is_success() {
            Ok(())
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// Generic request method (no body) with automatic 401 retry
    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let method_str = method.as_str().to_string();

        let mut request = self.client.request(method.clone(), &url);
        request = self.add_auth_header(request).await?;

        if let Some(b) = body {
            request = request.json(b);
        }

        let response = self.send_observed(&method_str, path, request).await?;
        let status = response.status();

        // If 401, try refreshing token and retry once
        if self.is_token_expired_error(status) && self.force_refresh_token().await.is_ok() {
            debug!(method = %method, url = %url, "Got 401, refreshing token and retrying");

            let mut request = self.client.request(method.clone(), &url);
            request = self.add_auth_header(request).await?;

            if let Some(b) = body {
                request = request.json(b);
            }

            let response = self.send_observed(&method_str, path, request).await?;
            return self.handle_response(response).await;
        }

        self.handle_response(response).await
    }

    /// Generic request method with body (supports slices) with automatic 401 retry
    async fn request_with_body<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let method_str = method.as_str().to_string();

        let mut request = self.client.request(method.clone(), &url);
        request = self.add_auth_header(request).await?;
        request = request.json(body);

        let response = self.send_observed(&method_str, path, request).await?;
        let status = response.status();

        // If 401, try refreshing token and retry once
        if self.is_token_expired_error(status) && self.force_refresh_token().await.is_ok() {
            debug!(method = %method, url = %url, "Got 401, refreshing token and retrying");

            let mut request = self.client.request(method.clone(), &url);
            request = self.add_auth_header(request).await?;
            request = request.json(body);

            let response = self.send_observed(&method_str, path, request).await?;
            return self.handle_response(response).await;
        }

        self.handle_response(response).await
    }

    /// Handle successful response, deserializing the body
    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            // Reset auth failures on successful request
            *self.auth_failures.write().await = 0;

            let bytes = response.bytes().await.map_err(ConductorError::Http)?;
            let text = String::from_utf8_lossy(&bytes);

            if text.trim().is_empty() {
                // Try to deserialize from empty - works for Option<T> or ()
                serde_json::from_str("null").map_err(ConductorError::Json)
            } else {
                serde_json::from_str(&text).map_err(|e| {
                    error!(body = %text, error = %e, "Failed to parse response");
                    ConductorError::Json(e)
                })
            }
        } else {
            Err(self.handle_error_response(response).await)
        }
    }

    /// Check if a 401 response indicates an expired/invalid token that should trigger retry
    fn is_token_expired_error(&self, status: StatusCode) -> bool {
        status == StatusCode::UNAUTHORIZED
    }

    /// Handle error response
    async fn handle_error_response(&self, response: Response) -> ConductorError {
        let status = response.status();
        let status_code = status.as_u16();

        // Track auth failures for backoff
        if status == StatusCode::UNAUTHORIZED {
            let mut failures = self.auth_failures.write().await;
            *failures += 1;
            warn!(failures = *failures, "Authentication failure");
        }

        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        match status {
            StatusCode::UNAUTHORIZED => ConductorError::auth(format!("Unauthorized: {}", message)),
            StatusCode::NOT_FOUND => ConductorError::api(format!("Not found: {}", message), None),
            StatusCode::BAD_REQUEST => {
                ConductorError::api(format!("Bad request: {}", message), None)
            }
            _ => ConductorError::server(status_code, message),
        }
    }

    /// Add authentication header to request
    ///
    /// Uses a mutex to ensure only one token refresh happens at a time,
    /// preventing thundering herd on token expiration. Also proactively
    /// refreshes tokens before TTL expiration.
    async fn add_auth_header(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        // First check current state (fast path, no lock needed for read)
        {
            let config = self.config.read().await;

            // Auth disabled (e.g., OSS Conductor) - return request as-is
            if config.auth_disabled {
                return Ok(request);
            }

            // No auth credentials configured - return request as-is
            if !config.has_auth() {
                return Ok(request);
            }

            // Have a valid token that doesn't need refresh - use it
            if let Some(ref token) = config.auth_token {
                if !config.token_needs_refresh() {
                    return Ok(request.header("X-Authorization", token));
                }
            }
        }

        // Need to refresh token - acquire the refresh lock to prevent concurrent refreshes
        let _refresh_guard = self.token_refresh_lock.lock().await;

        // Double-check if another thread already refreshed the token while we waited
        {
            let config = self.config.read().await;
            if config.auth_disabled {
                return Ok(request);
            }
            if let Some(ref token) = config.auth_token {
                if !config.token_needs_refresh() {
                    return Ok(request.header("X-Authorization", token));
                }
            }
        }

        // Actually refresh the token
        match self.refresh_token().await {
            Ok(()) => {}
            Err(e) => {
                // Check if auth was disabled during refresh (404 from OSS Conductor)
                let config = self.config.read().await;
                if config.auth_disabled {
                    return Ok(request);
                }
                return Err(e);
            }
        }

        // Now get the new token
        let config = self.config.read().await;
        if let Some(ref token) = config.auth_token {
            return Ok(request.header("X-Authorization", token));
        }

        Ok(request)
    }

    /// Refresh authentication token
    ///
    /// Handles several scenarios:
    /// - Success: Updates token and resets failure counter
    /// - 404: OSS Conductor without auth - disables auth for future requests
    /// - 401: Invalid credentials - increments failure counter with backoff
    /// - Other errors: Network issues, etc.
    async fn refresh_token(&self) -> Result<()> {
        let config = self.config.read().await;

        // Check if auth is disabled
        if config.auth_disabled {
            return Ok(());
        }

        let (key, secret) = match (&config.auth_key, &config.auth_secret) {
            (Some(k), Some(s)) => (k.clone(), s.clone()),
            _ => return Ok(()),
        };

        // Check failure count and apply backoff
        let failures = *self.auth_failures.read().await;
        if failures >= MAX_AUTH_FAILURES {
            error!(
                failures = failures,
                max = MAX_AUTH_FAILURES,
                "Token refresh has failed too many times. Please check your CONDUCTOR_AUTH_KEY and CONDUCTOR_AUTH_SECRET."
            );
            return Err(ConductorError::auth(format!(
                "Token refresh failed {} times. Check your authentication credentials.",
                failures
            )));
        }

        // Calculate backoff based on failures (exponential: 1s, 2s, 4s, 8s, 16s)
        if failures > 0 {
            // Check time since last attempt
            if let Some(last_attempt) = *self.last_refresh_attempt.read().await {
                let backoff = Duration::from_secs(2u64.pow(failures.min(5)));
                let elapsed = last_attempt.elapsed();

                if elapsed < backoff {
                    let remaining = backoff - elapsed;
                    warn!(
                        failures = failures,
                        backoff_secs = backoff.as_secs(),
                        remaining_secs = remaining.as_secs(),
                        "Auth backoff active, waiting before retry"
                    );
                    tokio::time::sleep(remaining).await;
                }
            }
        }

        drop(config); // Release read lock

        // Update last refresh attempt time
        *self.last_refresh_attempt.write().await = Some(Instant::now());

        debug!("Refreshing authentication token");

        let url = format!("{}/token", self.base_url);
        let body = serde_json::json!({
            "keyId": key,
            "keySecret": secret
        });

        let response = match self
            .send_observed("POST", "/token", self.client.post(&url).json(&body))
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                *self.auth_failures.write().await += 1;
                error!(error = %e, "Network error during token refresh");
                return Err(e);
            }
        };

        let status = response.status();

        if status.is_success() {
            let token_response: TokenResponse = response.json().await?;
            let mut config = self.config.write().await;
            config.update_token(token_response.token);
            *self.auth_failures.write().await = 0;
            debug!("Token refreshed successfully");
            Ok(())
        } else if status == StatusCode::NOT_FOUND {
            // 404 indicates OSS Conductor without authentication endpoint
            // Disable auth to prevent future attempts
            info!(
                "Authentication endpoint /token not found (404). \
                Running in open mode without authentication (Conductor OSS)."
            );
            let mut config = self.config.write().await;
            config.disable_auth();
            *self.auth_failures.write().await = 0;
            *self.is_oss.write().await = Some(true);
            Ok(())
        } else if status == StatusCode::UNAUTHORIZED {
            // 401 from /token endpoint - invalid credentials
            let message = response.text().await.unwrap_or_default();
            let mut failures = self.auth_failures.write().await;
            *failures += 1;
            error!(
                status = %status,
                failures = *failures,
                "Authentication failed. Please check your CONDUCTOR_AUTH_KEY and CONDUCTOR_AUTH_SECRET."
            );
            Err(ConductorError::auth(format!(
                "Invalid credentials: {} (attempt {}/{})",
                message, *failures, MAX_AUTH_FAILURES
            )))
        } else {
            // Other errors
            let message = response.text().await.unwrap_or_default();
            *self.auth_failures.write().await += 1;
            error!(status = %status, message = %message, "Failed to refresh token");
            Err(ConductorError::auth(format!(
                "Failed to get token: {} - {}",
                status, message
            )))
        }
    }

    /// Force refresh the authentication token
    ///
    /// Called when a request fails with 401, indicating the token may have
    /// been invalidated server-side before TTL expiration.
    pub async fn force_refresh_token(&self) -> Result<()> {
        // Clear current token to force refresh
        {
            let mut config = self.config.write().await;
            config.clear_token();
        }

        // Acquire lock and refresh
        let _refresh_guard = self.token_refresh_lock.lock().await;
        self.refresh_token().await
    }

    /// Check whether the server is OSS Conductor (vs Orkes Enterprise).
    ///
    /// Probes `POST /token` with dummy credentials on first call and caches
    /// the result. OSS Conductor returns 404 (the endpoint does not exist);
    /// Enterprise returns a non-404 status (e.g. 401/403 for invalid creds).
    ///
    /// This mirrors the detection strategy used by the JavaScript SDK.
    pub async fn is_oss(&self) -> bool {
        // Fast path: already probed
        if let Some(cached) = *self.is_oss.read().await {
            return cached;
        }

        // Probe /token
        let url = format!("{}/token", self.base_url);
        let body = serde_json::json!({"keyId": "probe", "keySecret": "probe"});
        let is_oss = match self
            .send_observed("POST", "/token", self.client.post(&url).json(&body))
            .await
        {
            Ok(resp) => resp.status() == StatusCode::NOT_FOUND,
            Err(_) => false,
        };

        info!(
            "Server detection: {} (POST {} → {})",
            if is_oss { "OSS" } else { "Enterprise" },
            url,
            if is_oss { "404" } else { "non-404" }
        );

        *self.is_oss.write().await = Some(is_oss);
        is_oss
    }

    /// Get configuration (for reading settings)
    pub async fn get_config(&self) -> Configuration {
        self.config.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let client = ApiClient::new(config);
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.base_url(), "http://localhost:8080/api");
    }
}
