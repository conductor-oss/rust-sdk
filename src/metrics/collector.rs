// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Prometheus implementation of the canonical Conductor SDK metric catalog.
//!
//! Metric names, label names, label values, and types here are intentionally
//! identical to the Java, Go, and Python SDKs. See `sdk-metrics-harmonization.md`
//! at https://github.com/orkes-io/certification-cloud-util/blob/main/sdk-metrics-harmonization.md

use parking_lot::RwLock;
use prometheus::{CounterVec, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use crate::events::{
    PollCompleted, PollFailure, PollSkippedPaused, PollStarted, TaskExecutionCompleted,
    TaskExecutionFailure, TaskExecutionStarted, TaskRunnerEventsListener, TaskUpdateCompleted,
    TaskUpdateFailure, ThreadUncaughtException, WorkflowStartFailure, WorkflowStarted,
};
use crate::http::HttpMetricsObserver;

use super::MetricsSettings;

/// Canonical time histogram buckets — identical to Java/Go/Python SDKs.
///
/// These buckets are finer-grained at the millisecond range than Prometheus'
/// defaults, reflecting Conductor's sub-second worker poll/update latencies.
const SECONDS_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Prometheus metrics collector implementing the canonical Conductor SDK
/// metric catalog.
///
/// Also implements [`HttpMetricsObserver`] so the same collector can be
/// installed into [`ApiClient`](crate::http::ApiClient) and receive HTTP-level
/// observations for `http_api_client_request_seconds`.
pub struct MetricsCollector {
    settings: MetricsSettings,
    registry: Registry,

    // -- Counters --
    task_poll_total: CounterVec,
    task_poll_error_total: CounterVec,
    task_execution_started_total: CounterVec,
    task_execute_error_total: CounterVec,
    task_update_error_total: CounterVec,
    task_paused_total: CounterVec,
    task_ack_error_total: CounterVec,
    task_ack_failed_total: CounterVec,
    task_execution_queue_full_total: CounterVec,
    external_payload_used_total: CounterVec,
    thread_uncaught_exceptions_total: CounterVec,
    workflow_start_error_total: CounterVec,

    // -- Histograms --
    task_poll_time_seconds: HistogramVec,
    task_execute_time_seconds: HistogramVec,
    task_update_time_seconds: HistogramVec,
    http_api_client_request_seconds: HistogramVec,

    // -- Gauges --
    task_result_size_bytes: GaugeVec,
    workflow_input_size_bytes: GaugeVec,
    active_workers: GaugeVec,

    // Internal tracking — keeps the `active_workers` gauge in sync with the
    // real active-task count when started/completed/failed events arrive.
    active_task_counts: Arc<RwLock<HashMap<String, i64>>>,
}

/// Helper: build a `CounterVec` with the canonical namespace and label set,
/// panicking with a clear message if construction or registration fails.
fn make_counter(
    registry: &Registry,
    namespace: &str,
    name: &'static str,
    help: &'static str,
    labels: &[&str],
) -> CounterVec {
    let counter = CounterVec::new(Opts::new(name, help).namespace(namespace), labels)
        .unwrap_or_else(|e| panic!("Failed to create counter {name}: {e}"));
    registry
        .register(Box::new(counter.clone()))
        .unwrap_or_else(|e| panic!("Failed to register counter {name}: {e}"));
    counter
}

/// Helper: build a histogram with the canonical SDK buckets.
fn make_histogram(
    registry: &Registry,
    namespace: &str,
    name: &'static str,
    help: &'static str,
    labels: &[&str],
) -> HistogramVec {
    let histogram = HistogramVec::new(
        HistogramOpts::new(name, help)
            .namespace(namespace)
            .buckets(SECONDS_BUCKETS.to_vec()),
        labels,
    )
    .unwrap_or_else(|e| panic!("Failed to create histogram {name}: {e}"));
    registry
        .register(Box::new(histogram.clone()))
        .unwrap_or_else(|e| panic!("Failed to register histogram {name}: {e}"));
    histogram
}

/// Helper: build a gauge vector.
fn make_gauge(
    registry: &Registry,
    namespace: &str,
    name: &'static str,
    help: &'static str,
    labels: &[&str],
) -> GaugeVec {
    let gauge = GaugeVec::new(Opts::new(name, help).namespace(namespace), labels)
        .unwrap_or_else(|e| panic!("Failed to create gauge {name}: {e}"));
    registry
        .register(Box::new(gauge.clone()))
        .unwrap_or_else(|e| panic!("Failed to register gauge {name}: {e}"));
    gauge
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(settings: MetricsSettings) -> Self {
        let registry = Registry::new();
        let ns = &settings.namespace;

        // Counters
        let task_poll_total = make_counter(
            &registry,
            ns,
            "task_poll_total",
            "Total number of task poll attempts",
            &["taskType"],
        );
        let task_poll_error_total = make_counter(
            &registry,
            ns,
            "task_poll_error_total",
            "Total number of task poll errors",
            &["taskType", "exception"],
        );
        let task_execution_started_total = make_counter(
            &registry,
            ns,
            "task_execution_started_total",
            "Count of task executions started",
            &["taskType"],
        );
        let task_execute_error_total = make_counter(
            &registry,
            ns,
            "task_execute_error_total",
            "Total number of task execution errors",
            &["taskType", "exception"],
        );
        let task_update_error_total = make_counter(
            &registry,
            ns,
            "task_update_error_total",
            "Total number of task update errors",
            &["taskType", "exception"],
        );
        let task_paused_total = make_counter(
            &registry,
            ns,
            "task_paused_total",
            "Number of polls skipped because worker was paused",
            &["taskType"],
        );
        let task_ack_error_total = make_counter(
            &registry,
            ns,
            "task_ack_error_total",
            "Count of task acknowledgement errors (surface-only in rust-sdk)",
            &["taskType", "exception"],
        );
        let task_ack_failed_total = make_counter(
            &registry,
            ns,
            "task_ack_failed_total",
            "Count of task acknowledgement failures (surface-only in rust-sdk)",
            &["taskType"],
        );
        let task_execution_queue_full_total = make_counter(
            &registry,
            ns,
            "task_execution_queue_full_total",
            "Count of executions dropped because the local execution queue was full \
             (surface-only in rust-sdk; tokio `Semaphore` never rejects)",
            &["taskType"],
        );
        let external_payload_used_total = make_counter(
            &registry,
            ns,
            "external_payload_used_total",
            "Count of times an external payload store was used \
             (surface-only in rust-sdk; reserved for future large-payload support)",
            &["entityName", "operation", "payloadType"],
        );
        let thread_uncaught_exceptions_total = make_counter(
            &registry,
            ns,
            "thread_uncaught_exceptions_total",
            "Count of panics escaping worker task bodies",
            &["exception"],
        );
        let workflow_start_error_total = make_counter(
            &registry,
            ns,
            "workflow_start_error_total",
            "Count of WorkflowClient::start_workflow failures",
            &["workflowType", "exception"],
        );

        // Histograms
        let task_poll_time_seconds = make_histogram(
            &registry,
            ns,
            "task_poll_time_seconds",
            "Task poll latency in seconds",
            &["taskType", "status"],
        );
        let task_execute_time_seconds = make_histogram(
            &registry,
            ns,
            "task_execute_time_seconds",
            "Task execution time in seconds",
            &["taskType", "status"],
        );
        let task_update_time_seconds = make_histogram(
            &registry,
            ns,
            "task_update_time_seconds",
            "Task update latency in seconds",
            &["taskType", "status"],
        );
        let http_api_client_request_seconds = make_histogram(
            &registry,
            ns,
            "http_api_client_request_seconds",
            "Conductor API HTTP client request latency in seconds",
            &["method", "uri", "status"],
        );

        // Gauges
        let task_result_size_bytes = make_gauge(
            &registry,
            ns,
            "task_result_size_bytes",
            "Size of task result payload in bytes",
            &["taskType"],
        );
        let workflow_input_size_bytes = make_gauge(
            &registry,
            ns,
            "workflow_input_size_bytes",
            "Size of workflow input payload in bytes at start_workflow time",
            &["workflowType", "version"],
        );
        let active_workers = make_gauge(
            &registry,
            ns,
            "active_workers",
            "Number of in-flight task executions",
            &["taskType"],
        );

        Self {
            settings,
            registry,
            task_poll_total,
            task_poll_error_total,
            task_execution_started_total,
            task_execute_error_total,
            task_update_error_total,
            task_paused_total,
            task_ack_error_total,
            task_ack_failed_total,
            task_execution_queue_full_total,
            external_payload_used_total,
            thread_uncaught_exceptions_total,
            workflow_start_error_total,
            task_poll_time_seconds,
            task_execute_time_seconds,
            task_update_time_seconds,
            http_api_client_request_seconds,
            task_result_size_bytes,
            workflow_input_size_bytes,
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

    /// Set the active worker count for a task type.
    pub fn set_active_workers(&self, task_type: &str, count: f64) {
        self.active_workers
            .with_label_values(&[task_type])
            .set(count);
    }

    /// Surface-only: increment `task_ack_error_total`. The current rust-sdk
    /// does not perform a separate ack RPC (poll returns tasks directly), so
    /// this counter is registered to keep the metric surface identical to
    /// Java/Go/Python but is never incremented by the SDK itself. Kept
    /// available for user code that performs its own acknowledgement flow.
    pub fn increment_task_ack_error(&self, task_type: &str, exception: &str) {
        self.task_ack_error_total
            .with_label_values(&[task_type, exception])
            .inc();
    }

    /// Surface-only: increment `task_ack_failed_total`.
    pub fn increment_task_ack_failed(&self, task_type: &str) {
        self.task_ack_failed_total
            .with_label_values(&[task_type])
            .inc();
    }

    /// Surface-only: increment `task_execution_queue_full_total`.
    pub fn increment_task_execution_queue_full(&self, task_type: &str) {
        self.task_execution_queue_full_total
            .with_label_values(&[task_type])
            .inc();
    }

    /// Surface-only: increment `external_payload_used_total`. Reserved for
    /// future large-payload external-storage support.
    pub fn increment_external_payload_used(
        &self,
        entity_name: &str,
        operation: &str,
        payload_type: &str,
    ) {
        self.external_payload_used_total
            .with_label_values(&[entity_name, operation, payload_type])
            .inc();
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
                                        Ok(()) => Response::builder()
                                            .status(200)
                                            .header(
                                                "Content-Type",
                                                "text/plain; charset=utf-8",
                                            )
                                            .body(Body::from(buffer))
                                            .unwrap_or_else(|e| {
                                                error!(error = %e, "Failed to build metrics response");
                                                Response::new(Body::from("Internal Server Error"))
                                            }),
                                        Err(e) => {
                                            error!(error = %e, "Failed to encode metrics");
                                            Response::builder()
                                                .status(500)
                                                .body(Body::from("Internal Server Error"))
                                                .unwrap_or_else(|_| {
                                                    Response::new(Body::from(
                                                        "Internal Server Error",
                                                    ))
                                                })
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

/// Canonical `status` label values. Match Java `Status.SUCCESS` / `Status.FAILURE`.
const STATUS_SUCCESS: &str = "SUCCESS";
const STATUS_FAILURE: &str = "FAILURE";

impl TaskRunnerEventsListener for MetricsCollector {
    fn on_poll_started(&self, event: &PollStarted) {
        self.task_poll_total
            .with_label_values(&[&event.task_type])
            .inc();
    }

    fn on_poll_completed(&self, event: &PollCompleted) {
        self.task_poll_time_seconds
            .with_label_values(&[&event.task_type, STATUS_SUCCESS])
            .observe(event.duration.as_secs_f64());
    }

    fn on_poll_failure(&self, event: &PollFailure) {
        self.task_poll_time_seconds
            .with_label_values(&[&event.task_type, STATUS_FAILURE])
            .observe(event.duration.as_secs_f64());

        self.task_poll_error_total
            .with_label_values(&[&event.task_type, &event.exception])
            .inc();
    }

    fn on_poll_skipped_paused(&self, event: &PollSkippedPaused) {
        self.task_paused_total
            .with_label_values(&[&event.task_type])
            .inc();
    }

    fn on_task_execution_started(&self, event: &TaskExecutionStarted) {
        self.task_execution_started_total
            .with_label_values(&[&event.task_type])
            .inc();

        let mut counts = self.active_task_counts.write();
        let count = counts.entry(event.task_type.clone()).or_insert(0);
        *count += 1;
        self.active_workers
            .with_label_values(&[&event.task_type])
            .set(*count as f64);
    }

    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        self.task_execute_time_seconds
            .with_label_values(&[&event.task_type, STATUS_SUCCESS])
            .observe(event.duration.as_secs_f64());

        if let Some(size) = event.output_size_bytes {
            self.task_result_size_bytes
                .with_label_values(&[&event.task_type])
                .set(size as f64);
        }

        self.decrement_active(&event.task_type);
    }

    fn on_task_execution_failure(&self, event: &TaskExecutionFailure) {
        self.task_execute_time_seconds
            .with_label_values(&[&event.task_type, STATUS_FAILURE])
            .observe(event.duration.as_secs_f64());

        self.task_execute_error_total
            .with_label_values(&[&event.task_type, &event.exception])
            .inc();

        self.decrement_active(&event.task_type);
    }

    fn on_task_update_completed(&self, event: &TaskUpdateCompleted) {
        self.task_update_time_seconds
            .with_label_values(&[&event.task_type, STATUS_SUCCESS])
            .observe(event.duration.as_secs_f64());
    }

    fn on_task_update_failure(&self, event: &TaskUpdateFailure) {
        self.task_update_time_seconds
            .with_label_values(&[&event.task_type, STATUS_FAILURE])
            .observe(event.duration.as_secs_f64());

        self.task_update_error_total
            .with_label_values(&[&event.task_type, &event.exception])
            .inc();
    }

    fn on_thread_uncaught_exception(&self, event: &ThreadUncaughtException) {
        self.thread_uncaught_exceptions_total
            .with_label_values(&[&event.exception])
            .inc();
    }

    fn on_workflow_started(&self, event: &WorkflowStarted) {
        let version_str = event
            .version
            .map(|v| v.to_string())
            .unwrap_or_default();
        self.workflow_input_size_bytes
            .with_label_values(&[&event.workflow_type, &version_str])
            .set(event.input_size_bytes as f64);
    }

    fn on_workflow_start_failure(&self, event: &WorkflowStartFailure) {
        self.workflow_start_error_total
            .with_label_values(&[&event.workflow_type, &event.exception])
            .inc();
    }
}

impl MetricsCollector {
    #[inline]
    fn decrement_active(&self, task_type: &str) {
        let mut counts = self.active_task_counts.write();
        if let Some(count) = counts.get_mut(task_type) {
            *count = (*count - 1).max(0);
            self.active_workers
                .with_label_values(&[task_type])
                .set(*count as f64);
        }
    }
}

impl HttpMetricsObserver for MetricsCollector {
    fn observe(&self, method: &str, uri: &str, status: &str, duration: Duration) {
        self.http_api_client_request_seconds
            .with_label_values(&[method, uri, status])
            .observe(duration.as_secs_f64());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        let event = PollStarted::new("test_task", "worker-1", 10);
        collector.on_poll_started(&event);

        let output = collector.gather();
        assert!(output.contains("task_poll_total"));
        assert!(
            !output.contains("conductor_task_poll_total"),
            "default namespace should be empty"
        );
    }

    #[test]
    fn test_poll_metrics() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        collector.on_poll_started(&PollStarted::new("test_task", "worker-1", 10));
        collector.on_poll_completed(&PollCompleted::new(
            "test_task",
            "worker-1",
            Duration::from_millis(50),
            5,
        ));

        let output = collector.gather();
        assert!(output.contains("task_poll_total"));
        assert!(output.contains("test_task"));
        assert!(output.contains("taskType=\"test_task\""));
        assert!(output.contains("status=\"SUCCESS\""));
    }

    #[test]
    fn test_execution_metrics() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        collector.on_task_execution_started(&TaskExecutionStarted::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
        ));
        collector.on_task_execution_completed(&TaskExecutionCompleted::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
            Duration::from_millis(100),
            Some(1024),
        ));

        let output = collector.gather();
        assert!(output.contains("task_execute_time_seconds"));
        assert!(output.contains("task_result_size_bytes"));
        assert!(output.contains("task_execution_started_total"));
    }

    #[test]
    fn test_failure_metrics_use_exception_label() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        collector.on_task_execution_failure(&TaskExecutionFailure::new(
            "test_task",
            "task-1",
            "wf-1",
            "worker-1",
            Duration::from_millis(5),
            "boom",
            "Worker",
            true,
        ));

        let output = collector.gather();
        assert!(output.contains("task_execute_error_total"));
        assert!(output.contains("exception=\"Worker\""));
        assert!(output.contains("status=\"FAILURE\""));
    }

    #[test]
    fn test_workflow_metrics() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        collector.on_workflow_started(&WorkflowStarted::new("wf_a", Some(1), 128));
        collector.on_workflow_start_failure(&WorkflowStartFailure::new("wf_b", "Server"));

        let output = collector.gather();
        assert!(output.contains("workflow_input_size_bytes"));
        assert!(output.contains("workflowType=\"wf_a\""));
        assert!(output.contains("workflow_start_error_total"));
        assert!(output.contains("workflowType=\"wf_b\""));
        assert!(output.contains("exception=\"Server\""));
    }

    #[test]
    fn test_http_observer() {
        let collector = MetricsCollector::new(MetricsSettings::default());

        <MetricsCollector as HttpMetricsObserver>::observe(
            &collector,
            "GET",
            "/tasks/poll/batch/my_worker",
            "200",
            Duration::from_millis(12),
        );

        let output = collector.gather();
        assert!(output.contains("http_api_client_request_seconds"));
        assert!(output.contains("method=\"GET\""));
        assert!(output.contains("uri=\"/tasks/poll/batch/my_worker\""));
        assert!(output.contains("status=\"200\""));
    }

    #[test]
    fn test_poll_skipped_paused_metric() {
        let collector = MetricsCollector::new(MetricsSettings::default());
        collector.on_poll_skipped_paused(&PollSkippedPaused::new("paused_task", "worker-1"));
        let output = collector.gather();
        assert!(output.contains("task_paused_total"));
        assert!(output.contains("paused_task"));
    }

    #[test]
    fn test_namespace_prefix_when_set() {
        let collector = MetricsCollector::new(MetricsSettings::default().with_namespace("myapp"));
        collector.on_poll_started(&PollStarted::new("test_task", "worker-1", 10));
        let output = collector.gather();
        assert!(output.contains("myapp_task_poll_total"));
    }
}
