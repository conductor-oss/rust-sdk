// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::env;
use std::time::{Duration, Instant};

/// Main configuration for Conductor client
#[derive(Debug, Clone)]
pub struct Configuration {
    /// Conductor server API URL (e.g., "http://localhost:8080/api")
    pub server_api_url: String,

    /// UI host URL for viewing executions
    pub ui_host: String,

    /// Authentication key (optional)
    pub auth_key: Option<String>,

    /// Authentication secret (optional)
    pub auth_secret: Option<String>,

    /// Current authentication token (managed internally)
    pub auth_token: Option<String>,

    /// Time when the token was last updated
    pub token_update_time: Option<Instant>,

    /// Token time-to-live duration (default: 45 minutes)
    pub auth_token_ttl: Duration,

    /// Whether authentication is disabled (e.g., OSS Conductor without auth)
    pub auth_disabled: bool,

    /// Request timeout
    pub timeout: Duration,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Enable debug logging
    pub debug: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Configuration {
    /// Default token TTL in minutes (45 minutes, same as Python SDK)
    const DEFAULT_TOKEN_TTL_MINUTES: u64 = 45;

    /// Create configuration from environment variables
    ///
    /// Environment variables:
    /// - `CONDUCTOR_SERVER_URL`: Conductor server API URL (default: http://localhost:8080/api)
    /// - `CONDUCTOR_AUTH_KEY`: Authentication key ID for Orkes Conductor
    /// - `CONDUCTOR_AUTH_SECRET`: Authentication secret for Orkes Conductor
    /// - `CONDUCTOR_DEBUG`: Enable debug mode (true/false)
    /// - `CONDUCTOR_TIMEOUT_SECS`: Request timeout in seconds (default: 30)
    /// - `CONDUCTOR_AUTH_TOKEN_TTL_MINS`: Token TTL in minutes (default: 45)
    ///
    /// The SDK automatically handles token refresh when using Orkes Conductor.
    /// For open-source Conductor without authentication, simply omit the auth variables.
    pub fn from_env() -> Self {
        // Load .env file if present
        let _ = dotenvy::dotenv();

        let server_api_url = env::var("CONDUCTOR_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:8080/api".to_string());

        // Derive UI host from server URL
        let ui_host =
            env::var("CONDUCTOR_UI_SERVER_URL").unwrap_or_else(|_| derive_ui_host(&server_api_url));

        let auth_key = env::var("CONDUCTOR_AUTH_KEY").ok();
        let auth_secret = env::var("CONDUCTOR_AUTH_SECRET").ok();

        let debug = env::var("CONDUCTOR_DEBUG")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false);

        let timeout_secs = env::var("CONDUCTOR_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let token_ttl_mins = env::var("CONDUCTOR_AUTH_TOKEN_TTL_MINS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(Self::DEFAULT_TOKEN_TTL_MINUTES);

        Self {
            server_api_url,
            ui_host,
            auth_key,
            auth_secret,
            auth_token: None,
            token_update_time: None,
            auth_token_ttl: Duration::from_secs(token_ttl_mins * 60),
            auth_disabled: false,
            timeout: Duration::from_secs(timeout_secs),
            connect_timeout: Duration::from_secs(10),
            debug,
        }
    }

    /// Create a new configuration with custom settings
    pub fn new(server_api_url: impl Into<String>) -> Self {
        let server_api_url = server_api_url.into();
        let ui_host = derive_ui_host(&server_api_url);

        Self {
            server_api_url,
            ui_host,
            auth_key: None,
            auth_secret: None,
            auth_token: None,
            token_update_time: None,
            auth_token_ttl: Duration::from_secs(Self::DEFAULT_TOKEN_TTL_MINUTES * 60),
            auth_disabled: false,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            debug: false,
        }
    }

    /// Set authentication credentials
    pub fn with_auth(mut self, key: impl Into<String>, secret: impl Into<String>) -> Self {
        self.auth_key = Some(key.into());
        self.auth_secret = Some(secret.into());
        self
    }

    /// Set token TTL (time-to-live) duration
    pub fn with_token_ttl(mut self, ttl: Duration) -> Self {
        self.auth_token_ttl = ttl;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Update the authentication token
    pub fn update_token(&mut self, token: String) {
        self.auth_token = Some(token);
        self.token_update_time = Some(Instant::now());
    }

    /// Clear the authentication token (e.g., after auth failure)
    pub fn clear_token(&mut self) {
        self.auth_token = None;
        self.token_update_time = None;
    }

    /// Disable authentication (e.g., when server doesn't require it)
    pub fn disable_auth(&mut self) {
        self.auth_disabled = true;
        self.auth_key = None;
        self.auth_secret = None;
        self.auth_token = None;
        self.token_update_time = None;
    }

    /// Check if authentication is configured and not disabled
    pub fn has_auth(&self) -> bool {
        !self.auth_disabled && self.auth_key.is_some() && self.auth_secret.is_some()
    }

    /// Check if the token needs to be refreshed based on TTL
    pub fn token_needs_refresh(&self) -> bool {
        // No token yet
        if self.auth_token.is_none() {
            return true;
        }

        // Check if token has expired based on TTL
        if let Some(update_time) = self.token_update_time {
            let elapsed = update_time.elapsed();
            return elapsed >= self.auth_token_ttl;
        }

        // No update time recorded, needs refresh
        true
    }

    /// Get the execution URL for a workflow
    pub fn execution_url(&self, workflow_id: &str) -> String {
        format!("{}/execution/{}", self.ui_host, workflow_id)
    }
}

/// Derive UI host from server API URL
fn derive_ui_host(server_url: &str) -> String {
    // Remove /api suffix if present
    let base = server_url.trim_end_matches("/api").trim_end_matches('/');

    // If it's localhost:8080, UI is typically on :5000
    if base.contains("localhost:8080") || base.contains("127.0.0.1:8080") {
        base.replace(":8080", ":5000")
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Configuration::new("http://localhost:8080/api");
        assert!(config.server_api_url.contains("localhost:8080"));
        assert!(!config.auth_disabled);
        assert_eq!(config.auth_token_ttl, Duration::from_secs(45 * 60));
    }

    #[test]
    fn test_derive_ui_host() {
        assert_eq!(
            derive_ui_host("http://localhost:8080/api"),
            "http://localhost:5000"
        );
        assert_eq!(
            derive_ui_host("https://conductor.example.com/api"),
            "https://conductor.example.com"
        );
    }

    #[test]
    fn test_execution_url() {
        let config = Configuration::new("http://localhost:8080/api");
        assert_eq!(
            config.execution_url("abc123"),
            "http://localhost:5000/execution/abc123"
        );
    }

    #[test]
    fn test_with_auth() {
        let config =
            Configuration::new("http://localhost:8080/api").with_auth("key123", "secret456");

        assert!(config.has_auth());
        assert_eq!(config.auth_key, Some("key123".to_string()));
        assert_eq!(config.auth_secret, Some("secret456".to_string()));
    }

    #[test]
    fn test_token_needs_refresh() {
        let mut config = Configuration::new("http://localhost:8080/api");

        // No token - needs refresh
        assert!(config.token_needs_refresh());

        // Add token
        config.update_token("test-token".to_string());

        // Just updated - doesn't need refresh
        assert!(!config.token_needs_refresh());

        // Simulate expired token by setting a very short TTL
        config.auth_token_ttl = Duration::from_millis(1);
        std::thread::sleep(Duration::from_millis(10));

        // Now needs refresh
        assert!(config.token_needs_refresh());
    }

    #[test]
    fn test_disable_auth() {
        let mut config = Configuration::new("http://localhost:8080/api").with_auth("key", "secret");

        assert!(config.has_auth());

        config.update_token("test-token".to_string());
        assert!(config.auth_token.is_some());

        config.disable_auth();

        assert!(!config.has_auth());
        assert!(config.auth_disabled);
        assert!(config.auth_key.is_none());
        assert!(config.auth_secret.is_none());
        assert!(config.auth_token.is_none());
    }

    #[test]
    fn test_clear_token() {
        let mut config = Configuration::new("http://localhost:8080/api");
        config.update_token("test-token".to_string());

        assert!(config.auth_token.is_some());
        assert!(config.token_update_time.is_some());

        config.clear_token();

        assert!(config.auth_token.is_none());
        assert!(config.token_update_time.is_none());
    }

    #[test]
    fn test_with_token_ttl() {
        let config = Configuration::new("http://localhost:8080/api")
            .with_token_ttl(Duration::from_secs(30 * 60)); // 30 minutes

        assert_eq!(config.auth_token_ttl, Duration::from_secs(30 * 60));
    }
}
