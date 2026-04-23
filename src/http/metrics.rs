// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! HTTP client metrics observer trait.
//!
//! The [`ApiClient`](super::ApiClient) invokes a trait object of
//! [`HttpMetricsObserver`] on every outbound request so that instrumentation
//! (e.g. the [`MetricsCollector`](crate::metrics::MetricsCollector) from the
//! `metrics` module) can record `http_api_client_request_seconds` without the
//! HTTP layer depending on the metrics layer.

use std::sync::Arc;
use std::time::Duration;

/// Observer invoked by [`ApiClient`](super::ApiClient) after every request
/// completes (either with a response or a transport error).
///
/// Implementations should be fast and non-blocking — the observer runs on the
/// request hot-path.
pub trait HttpMetricsObserver: Send + Sync {
    /// Record a completed HTTP request.
    ///
    /// - `method`: uppercase HTTP verb (e.g. `"GET"`).
    /// - `uri`: interpolated request path, *without* query string (e.g.
    ///   `/tasks/poll/batch/my_worker`). Template extraction is tracked as
    ///   Phase 4 of the canonical SDK metrics harmonization plan.
    /// - `status`: HTTP status code as a string, or `"0"` if the transport
    ///   failed before a status was received.
    /// - `duration`: wall-clock time from send to response-received (or error).
    fn observe(&self, method: &str, uri: &str, status: &str, duration: Duration);
}

/// No-op observer installed by default.
pub struct NoopHttpMetricsObserver;

impl HttpMetricsObserver for NoopHttpMetricsObserver {
    fn observe(&self, _method: &str, _uri: &str, _status: &str, _duration: Duration) {}
}

impl NoopHttpMetricsObserver {
    /// Return a shared no-op observer instance.
    pub fn arc() -> Arc<dyn HttpMetricsObserver> {
        Arc::new(Self)
    }
}
