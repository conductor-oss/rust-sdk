// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! HTTP client metrics observer trait.
//!
//! The [`ApiClient`](super::ApiClient) invokes a trait object of
//! [`HttpMetricsObserver`] on every outbound request so that instrumentation
//! (e.g. the [`MetricsCollector`](crate::metrics::MetricsCollector) from the
//! `metrics` module) can record `http_api_client_request_seconds` without the
//! HTTP layer depending on the metrics layer.
//!
//! The `uri` value passed to [`HttpMetricsObserver::observe`] is a
//! bounded-cardinality **path template** (e.g. `/tasks/poll/batch/{taskType}`)
//! rather than the interpolated request path. The server base-path prefix
//! (e.g. `/api`) is not included.

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
    /// - `uri`: bounded-cardinality path template, *without* query string
    ///   (e.g. `/tasks/poll/batch/{taskType}`). Dynamic segments such as
    ///   workflow IDs or task names are replaced by `{placeholder}` tokens.
    ///   The server base-path prefix (e.g. `/api`) is **not** included.
    /// - `status`: HTTP status code as a string, or `"0"` if the transport
    ///   failed before a status was received.
    /// - `duration`: wall-clock time from send to response-received (or error).
    fn observe(&self, method: &str, uri: &str, status: &str, duration: Duration);
}
