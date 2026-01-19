//! HTTP client for Conductor API
//!
//! Provides async HTTP client with:
//! - Automatic authentication
//! - Request/response logging
//! - Error handling
//! - Retry logic

mod api_client;

pub use api_client::ApiClient;
