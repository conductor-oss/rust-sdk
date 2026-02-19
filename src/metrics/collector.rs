// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use parking_lot::RwLock;
use prometheus::{CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

use crate::events::{
    PollCompleted, PollFailure, PollStarted, TaskExecutionCompleted, TaskExecutionFailure,
    TaskExecutionStarted, TaskRunnerEventsListener, TaskUpdateFailure,
};

use super::MetricsSettings;

/// Prometheus metrics collector implementing the TaskRunnerEventsListener trait
pub struct MetricsCollector {
    settings: MetricsSettings,
    registry: Registry,

    // Counters
    task_poll_total: CounterVec,
    task_poll_error_total: CounterVec,
    task_execute_error_total: CounterVec,
    task_update_error_total: CounterVec,
    task_paused_total: CounterVec,

    // Histograms
    task_poll_time_seconds: HistogramVec,
    task_execute_time_seconds: HistogramVec,
    #[allow(dead_code)] // Registered for future use when task update success events are added
    task_update_time_seconds: HistogramVec,

    // Gauges
    task_result_size_bytes: GaugeVec,
    active_workers: GaugeVec,

    // Internal tracking
    active_task_counts: Arc<RwLock<HashMap<String, i64>>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(settings: MetricsSettings) -> Self {
        let registry = Registry::new();
        let namespace = &settings.namespace;

        // Create counters
        let task_poll_total = CounterVec::new(
            Opts::new("task_poll_total", "Total number of task poll attempts").namespace(namespace),
            &["task_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_poll_total counter: {}", e);
        });

        let task_poll_error_total = CounterVec::new(
            Opts::new("task_poll_error_total", "Total number of task poll errors")
                .namespace(namespace),
            &["task_type", "error_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_poll_error_total counter: {}", e);
        });

        let task_execute_error_total = CounterVec::new(
            Opts::new(
                "task_execute_error_total",
                "Total number of task execution errors",
            )
            .namespace(namespace),
            &["task_type", "error_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_execute_error_total counter: {}", e);
        });

        let task_update_error_total = CounterVec::new(
            Opts::new(
                "task_update_error_total",
                "Total number of task update errors",
            )
            .namespace(namespace),
            &["task_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_update_error_total counter: {}", e);
        });

        let task_paused_total = CounterVec::new(
            Opts::new("task_paused_total", "Number of polls while worker paused")
                .namespace(namespace),
            &["task_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_paused_total counter: {}", e);
        });

        // Create histograms with default buckets
        let buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
        ];

        let task_poll_time_seconds = HistogramVec::new(
            HistogramOpts::new("task_poll_time_seconds", "Task poll latency in seconds")
                .namespace(namespace)
                .buckets(buckets.clone()),
            &["task_type", "status"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_poll_time_seconds histogram: {}", e);
        });

        let task_execute_time_seconds = HistogramVec::new(
            HistogramOpts::new(
                "task_execute_time_seconds",
                "Task execution time in seconds",
            )
            .namespace(namespace)
            .buckets(buckets.clone()),
            &["task_type", "status"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_execute_time_seconds histogram: {}", e);
        });

        let task_update_time_seconds = HistogramVec::new(
            HistogramOpts::new("task_update_time_seconds", "Task update latency in seconds")
                .namespace(namespace)
                .buckets(buckets),
            &["task_type", "status"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_update_time_seconds histogram: {}", e);
        });

        // Create gauges
        let task_result_size_bytes = GaugeVec::new(
            Opts::new("task_result_size_bytes", "Size of task result payload").namespace(namespace),
            &["task_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create task_result_size_bytes gauge: {}", e);
        });

        let active_workers = GaugeVec::new(
            Opts::new("active_workers", "Number of active workers").namespace(namespace),
            &["task_type"],
        )
        .unwrap_or_else(|e| {
            panic!("Failed to create active_workers gauge: {}", e);
        });

        // Register metrics
        if let Err(e) = registry.register(Box::new(task_poll_total.clone())) {
            panic!("Failed to register task_poll_total: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_poll_error_total.clone())) {
            panic!("Failed to register task_poll_error_total: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_execute_error_total.clone())) {
            panic!("Failed to register task_execute_error_total: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_update_error_total.clone())) {
            panic!("Failed to register task_update_error_total: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_paused_total.clone())) {
            panic!("Failed to register task_paused_total: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_poll_time_seconds.clone())) {
            panic!("Failed to register task_poll_time_seconds: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_execute_time_seconds.clone())) {
            panic!("Failed to register task_execute_time_seconds: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_update_time_seconds.clone())) {
            panic!("Failed to register task_update_time_seconds: {}", e);
        }
        if let Err(e) = registry.register(Box::new(task_result_size_bytes.clone())) {
            panic!("Failed to register task_result_size_bytes: {}", e);
        }
        if let Err(e) = registry.register(Box::new(active_workers.clone())) {
            panic!("Failed to register active_workers: {}", e);
        }

        Self {
            settings,
            registry,
            task_poll_total,
            task_poll_error_total,
            task_execute_error_total,
            task_update_error_total,
            task_paused_total,
            task_poll_time_seconds,
            task_execute_time_seconds,
            task_update_time_seconds,
            task_result_size_bytes,
            active_workers,
            active_task_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get metrics in Prometheus text format
    pub fn gather(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
            error!(error = %e, "Failed to encode metrics");
            return String::new();
        }
        String::from_utf8(buffer).unwrap_or_else(|e| {
            error!(error = %e, "Failed to convert metrics buffer to UTF-8");
            String::new()
        })
    }

    /// Increment task paused counter
    pub fn increment_task_paused(&self, task_type: &str) {
        self.task_paused_total.with_label_values(&[task_type]).inc();
    }

    /// Set active worker count
    pub fn set_active_workers(&self, task_type: &str, count: f64) {
        self.active_workers
            .with_label_values(&[task_type])
            .set(count);
    }

    /// Start HTTP metrics server (if configured)
    pub async fn start_http_server(&self) -> Option<tokio::task::JoinHandle<()>> {
        if let Some(port) = self.settings.http_port {
            let metrics_path = self.settings.metrics_path.clone();
            let health_path = self.settings.health_path.clone();
            let registry = self.registry.clone();

            let handle = tokio::spawn(async move {
                use hyper::service::{make_service_fn, service_fn};
                use hyper::{Body, Request, Response, Server};
                use std::convert::Infallible;
                use std::net::SocketAddr;

                let addr = SocketAddr::from(([0, 0, 0, 0], port));

                let make_service = make_service_fn(move |_| {
                    let registry = registry.clone();
                    let metrics_path = metrics_path.clone();
                    let health_path = health_path.clone();

                    async move {
                        Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                            let registry = registry.clone();
                            let metrics_path = metrics_path.clone();
                            let health_path = health_path.clone();

                            async move {
                                let response = if req.uri().path() == metrics_path {
                                    use prometheus::Encoder;
                                    let encoder = prometheus::TextEncoder::new();
                                    let metric_families = registry.gather();
                                    let mut buffer = Vec::new();
                                    
                                    match encoder.encode(&metric_families, &mut buffer) {
                                        Ok(()) => {
                                            Response::builder()
                                                .status(200)
                                                .header("Content-Type", "text/plain; charset=utf-8")
                                                .body(Body::from(buffer))
                                                .unwrap_or_else(|e| {
                                                    error!(error = %e, "Failed to build metrics response");
                                                    Response::new(Body::from("Internal Server Error"))
                                                })
                                        }
                                        Err(e) => {
                                            error!(error = %e, "Failed to encode metrics");
                                            Response::builder()
                                                .status(500)
                                                .body(Body::from("Internal Server Error"))
                                                .unwrap_or_else(|_| Response::new(Body::from("Internal Server Error")))
                                        }
                                    }
                                } else if req.uri().path() == health_path {
                                    Response::builder()
                                        .status(200)
                                        .body(Body::from("OK"))
                                        .unwrap_or_else(|e| {
                                            error!(error = %e, "Failed to build health response");
                                            Response::new(Body::from("Internal Server Error"))
                                        })
                                } else {
                                    Response::builder()
                                        .status(404)
                                        .body(Body::from("Not Found"))
                                        .unwrap_or_else(|e| {
                                            error!(error = %e, "Failed to build 404 response");
                                            Response::new(Body::from("Not Found"))
                                        })
                                };
                                Ok::<_, Infallible>(response)
                            }
                        }))
                    }
                });

                info!(port = port, "Starting metrics HTTP server");

                if let Err(e) = Server::bind(&addr).serve(make_service).await {
                    error!(error = %e, "Metrics HTTP server error");
                }
            });

            Some(handle)
        } else {
            None
        }
    }
}

impl TaskRunnerEventsListener for MetricsCollector {
    fn on_poll_started(&self, event: &PollStarted) {
        self.task_poll_total
            .with_label_values(&[&event.task_type])
            .inc();
    }

    fn on_poll_completed(&self, event: &PollCompleted) {
        self.task_poll_time_seconds
            .with_label_values(&[&event.task_type, "success"])
            .observe(event.duration.as_secs_f64());
    }

    fn on_poll_failure(&self, event: &PollFailure) {
        self.task_poll_time_seconds
            .with_label_values(&[&event.task_type, "failure"])
            .observe(event.duration.as_secs_f64());

        self.task_poll_error_total
            .with_label_values(&[&event.task_type, "poll_error"])
            .inc();
    }

    fn on_task_execution_started(&self, event: &TaskExecutionStarted) {
        // Track active tasks
        let mut counts = self.active_task_counts.write();
        let count = counts.entry(event.task_type.clone()).or_insert(0);
        *count += 1;
        self.active_workers
            .with_label_values(&[&event.task_type])
            .set(*count as f64);
    }

    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        self.task_execute_time_seconds
            .with_label_values(&[&event.task_type, "success"])
            .observe(event.duration.as_secs_f64());

        if let Some(size) = event.output_size_bytes {
            self.task_result_size_bytes
                .with_label_values(&[&event.task_type])
                .set(size as f64);
        }

        // Track active tasks
        let mut counts = self.active_task_counts.write();
        if let Some(count) = counts.get_mut(&event.task_type) {
            *count = (*count - 1).max(0);
            self.active_workers
                .with_label_values(&[&event.task_type])
                .set(*count as f64);
        }
    }

    fn on_task_execution_failure(&self, event: &TaskExecutionFailure) {
        self.task_execute_time_seconds
            .with_label_values(&[&event.task_type, "failure"])
            .observe(event.duration.as_secs_f64());

        self.task_execute_error_total
            .with_label_values(&[&event.task_type, "execution_error"])
            .inc();

        // Track active tasks
        let mut counts = self.active_task_counts.write();
        if let Some(count) = counts.get_mut(&event.task_type) {
            *count = (*count - 1).max(0);
            self.active_workers
                .with_label_values(&[&event.task_type])
                .set(*count as f64);
        }
    }

    fn on_task_update_failure(&self, event: &TaskUpdateFailure) {
        self.task_update_error_total
            .with_label_values(&[&event.task_type])
            .inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_metrics_collector_creation() {
        let settings = MetricsSettings::default();
        let collector = MetricsCollector::new(settings);

        // Should be able to gather metrics (some labels need data first)
        let event = PollStarted::new("test_task", "worker-1", 10);
        collector.on_poll_started(&event);

        let output = collector.gather();
        assert!(output.contains("task_poll_total"));
    }

    #[test]
    fn test_poll_metrics() {
        let settings = MetricsSettings::default();
        let collector = MetricsCollector::new(settings);

        let event = PollStarted::new("test_task", "worker-1", 10);
        collector.on_poll_started(&event);

        let event = PollCompleted::new("test_task", "worker-1", Duration::from_millis(50), 5);
        collector.on_poll_completed(&event);

        let output = collector.gather();
        assert!(output.contains("conductor_task_poll_total"));
        assert!(output.contains("test_task"));
    }

    #[test]
    fn test_execution_metrics() {
        let settings = MetricsSettings::default();
        let collector = MetricsCollector::new(settings);

        let start_event = TaskExecutionStarted::new("test_task", "task-1", "wf-1", "worker-1");
        collector.on_task_execution_started(&start_event);

        let complete_event = TaskExecutionCompleted::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
            Duration::from_millis(100),
            Some(1024),
        );
        collector.on_task_execution_completed(&complete_event);

        let output = collector.gather();
        assert!(output.contains("conductor_task_execute_time_seconds"));
        assert!(output.contains("conductor_task_result_size_bytes"));
    }
}
