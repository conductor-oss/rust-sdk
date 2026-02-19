// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::time::Duration;

/// Settings for metrics collection
#[derive(Debug, Clone)]
pub struct MetricsSettings {
    /// Enable metrics collection
    pub enabled: bool,

    /// HTTP port for metrics endpoint (if Some, serves metrics via HTTP)
    pub http_port: Option<u16>,

    /// Metrics endpoint path (default: /metrics)
    pub metrics_path: String,

    /// Health endpoint path (default: /health)
    pub health_path: String,

    /// Update interval for metrics
    pub update_interval: Duration,

    /// Namespace prefix for all metrics
    pub namespace: String,
}

impl Default for MetricsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            http_port: None,
            metrics_path: "/metrics".to_string(),
            health_path: "/health".to_string(),
            update_interval: Duration::from_secs(1),
            namespace: "conductor".to_string(),
        }
    }
}

impl MetricsSettings {
    /// Create new metrics settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable HTTP metrics endpoint
    pub fn with_http_port(mut self, port: u16) -> Self {
        self.http_port = Some(port);
        self
    }

    /// Set metrics path
    pub fn with_metrics_path(mut self, path: impl Into<String>) -> Self {
        self.metrics_path = path.into();
        self
    }

    /// Set namespace
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    /// Set update interval
    pub fn with_update_interval(mut self, interval: Duration) -> Self {
        self.update_interval = interval;
        self
    }

    /// Disable metrics
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = MetricsSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.metrics_path, "/metrics");
        assert_eq!(settings.namespace, "conductor");
    }

    #[test]
    fn test_builder() {
        let settings = MetricsSettings::new()
            .with_http_port(9090)
            .with_namespace("myapp");

        assert_eq!(settings.http_port, Some(9090));
        assert_eq!(settings.namespace, "myapp");
    }
}
