# Metrics Documentation

The Conductor Rust SDK includes built-in metrics collection using Prometheus to
monitor worker performance, API requests, and task execution.

All metric names, label names, label values, and Prometheus types emitted by
this SDK match the canonical catalog in
[`sdk-metrics-harmonization.md`](https://github.com/orkes-io/certification-cloud-util/blob/main/sdk-metrics-harmonization.md).
Because the Rust SDK is unreleased, there are no legacy/deprecated metric
names to carry forward — the emitted surface is canonical on day one.

## Table of Contents

- [Quick Reference](#quick-reference)
- [Configuration](#configuration)
- [Intentional divergences](#intentional-divergences)
- [Examples](#examples)

## Quick Reference

### Canonical metrics emitted by the SDK

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `task_poll_total` | Counter | `taskType` | Incremented for every poll request issued to the server. |
| `task_poll_error_total` | Counter | `taskType`, `exception` | Client-side poll failures. `exception` is the unqualified `ConductorError` variant name. |
| `task_execution_started_total` | Counter | `taskType` | Incremented when a polled task is dispatched to the user worker function. |
| `task_execute_error_total` | Counter | `taskType`, `exception` | User worker returned `Err(_)`. |
| `task_update_error_total` | Counter | `taskType`, `exception` | Task-result update back to the server failed after all retries. |
| `task_paused_total` | Counter | `taskType` | Poll skipped because the runner is paused. |
| `thread_uncaught_exceptions_total` | Counter | `exception` | Panic escaped a spawned worker task; `exception` is always `"Panic"`. |
| `workflow_start_error_total` | Counter | `workflowType`, `exception` | `WorkflowClient::start_workflow` failed client-side. |
| `task_ack_error_total` | Counter | `taskType`, `exception` | **Surface-only.** Not incremented by the internal runner (see [Intentional divergences](#intentional-divergences)). |
| `task_ack_failed_total` | Counter | `taskType` | **Surface-only.** Not incremented by the internal runner. |
| `task_execution_queue_full_total` | Counter | `taskType` | **Surface-only.** Not incremented by the internal runner. |
| `external_payload_used_total` | Counter | `entityName`, `operation`, `payloadType` | **Surface-only.** Reserved for future large-payload external-storage support. |
| `task_poll_time_seconds` | Histogram | `taskType`, `status` | Poll latency. `status ∈ {SUCCESS, FAILURE}`. |
| `task_execute_time_seconds` | Histogram | `taskType`, `status` | User worker function wall-clock. |
| `task_update_time_seconds` | Histogram | `taskType`, `status` | Latency of the `UpdateTask` call (including retries). |
| `http_api_client_request_seconds` | Histogram | `method`, `uri`, `status` | Latency of every Conductor API HTTP request. `status` is the HTTP status code as a string, or `"0"` for network errors. |
| `task_result_size_bytes` | Gauge | `taskType` | Last-seen serialized task-result size. |
| `workflow_input_size_bytes` | Gauge | `workflowType`, `version` | Last-seen serialized `StartWorkflowRequest.input` size. `version` is the workflow version as a string, or `""` when unset. |
| `active_workers` | Gauge | `taskType` | Current number of in-flight task executions. |

The Histogram bucket set is the canonical
`(0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0)`
seconds.

### Label values

- `status` on task time histograms: uppercase `"SUCCESS"` / `"FAILURE"`.
- `status` on `http_api_client_request_seconds`: HTTP status code rendered as
  a string (e.g. `"200"`), or `"0"` when the transport layer fails before
  receiving a status.
- `uri`: the full interpolated request path (server path prefix + endpoint
  path) without query string (e.g. `/api/tasks/poll/batch/my_worker` when
  the server URL is `http://host:8080/api`). See the note below about
  Phase 4 path templating.
- `exception`: the unqualified `ConductorError` variant name
  (`Http`, `Json`, `Auth`, `Server`, …), the short type name for non-
  `ConductorError` errors, or `"Panic"` for uncaught panics.

### `uri` label — interpolated path, not templated

Like the Java / Go / Python SDKs in Phase 1 of the harmonization plan, the
`uri` label on `http_api_client_request_seconds` carries the **interpolated**
request path including the server URL's path prefix (e.g.
`/api/tasks/poll/batch/my_task` when `CONDUCTOR_SERVER_URL` ends in `/api`),
not the templated path (`/api/tasks/poll/batch/{taskType}`).
High-cardinality worker names or task IDs will therefore appear in the label.

Operators who need bounded cardinality today should apply a Prometheus
`metric_relabel_configs` rule at scrape time that rewrites well-known
parametric path segments. Template extraction is tracked as **Phase 4** of the
canonical SDK metrics harmonization plan.

## Configuration

Metrics are wired up by calling [`TaskHandler::enable_metrics`]. This:

- Registers a shared `MetricsCollector` as a `TaskRunnerEventsListener` for
  task-level events.
- Installs the same `MetricsCollector` as the `HttpMetricsObserver` inside the
  handler's `ApiClient`, capturing every HTTP request (including requests
  made by `ConductorClient` instances vended via `TaskHandler::conductor_client()`).
- Optionally starts an HTTP scrape endpoint (`/metrics`, `/health`).

Example:

```rust
use conductor::{
    configuration::Configuration,
    metrics::MetricsSettings,
    worker::TaskHandler,
};

let config = Configuration::from_env();
let mut handler = TaskHandler::new(config)?;

handler.enable_metrics(
    MetricsSettings::new()
        .with_http_port(9991)
        .with_metrics_path("/metrics"),
);

// Workflow-start events will flow through the same dispatcher as tasks:
let conductor = handler.conductor_client();
let workflow_client = conductor.workflow_client();
```

By default `MetricsSettings::namespace` is `""`, so metric names appear
uncurried (e.g. `task_poll_total`, matching Java/Go/Python). Call
`.with_namespace("myapp")` to prefix names if you need to isolate Conductor
SDK metrics from other metrics in the same registry.

## Intentional divergences

Some asymmetries with the canonical catalog are kept by design rather than
papered over:

| Metric | Status in Rust SDK | Reason |
|---|---|---|
| `task_ack_error_total`, `task_ack_failed_total` | Registered; never incremented by the internal runner. Public helpers `MetricsCollector::increment_task_ack_error` / `increment_task_ack_failed` exposed for user code. | Matches the Go SDK's runtime model: the batch-poll response itself acts as the ack, so there is no separate ack call for the SDK to instrument. |
| `task_execution_queue_full_total` | Registered; never incremented by the internal runner. | Rust's worker scheduling uses a `tokio::sync::Semaphore`; acquisition awaits rather than rejecting, so there is no "queue full" condition for the SDK to surface. |
| `external_payload_used_total` | Registered; never incremented by the internal runner. | The Rust client does not yet integrate with the external-payload-storage branch of the Conductor API. Helper method retained for user code that implements its own external-payload plumbing. |
| `worker_restart_total` | Not emitted. | Python-only metric: Python has a multi-process worker supervisor; Rust spawns Tokio tasks, so there is no equivalent "restart a subprocess" event. |
| `task_execution_completed_total` | Not emitted. | Canonical catalog exposes task execution completion only through `task_execute_time_seconds_count{status="SUCCESS"}`, which is already present. |
| `active_workers` labels | `{taskType}` | Matches canonical. |
| Metric name prefix | `""` (none) by default | Matches Java/Go/Python. Can be overridden via `MetricsSettings::with_namespace`. |

## Examples

See [`examples/metrics_example.rs`](./examples/metrics_example.rs) for a
runnable end-to-end demo that spins up workers, serves `/metrics` on a
configurable port, and exercises every metric in the catalog.

```prometheus
# HTTP API client request latency
http_api_client_request_seconds_bucket{method="GET",uri="/tasks/poll/batch/my_worker",status="200",le="0.1"} 97
http_api_client_request_seconds_bucket{method="GET",uri="/tasks/poll/batch/my_worker",status="200",le="+Inf"} 100
http_api_client_request_seconds_count{method="GET",uri="/tasks/poll/batch/my_worker",status="200"} 100
http_api_client_request_seconds_sum{method="GET",uri="/tasks/poll/batch/my_worker",status="200"} 8.21

# Task poll
task_poll_total{taskType="my_worker"} 124

# Task execute time (SUCCESS)
task_execute_time_seconds_bucket{taskType="my_worker",status="SUCCESS",le="0.25"} 42
task_execute_time_seconds_count{taskType="my_worker",status="SUCCESS"} 42

# Workflow start error
workflow_start_error_total{workflowType="my_wf",exception="Server"} 2
```
