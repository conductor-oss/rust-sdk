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
- [Troubleshooting](#troubleshooting)
- [Detailed Technical Notes](#detailed-technical-notes)

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
| `task_result_size_bytes` | Histogram | `taskType` | Serialized byte size of task result output. |
| `workflow_input_size_bytes` | Histogram | `workflowType`, `version` | Serialized byte size of workflow input. `version` is the workflow version as a string, or `""` when unset. |
| `active_workers` | Gauge | `taskType` | Current number of in-flight task executions. |

Time histograms use the canonical seconds bucket set:
`(0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0)`.

Size histograms use the canonical size bucket set:
`(100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000)` bytes.

### Label values

- `status` on task time histograms: uppercase `"SUCCESS"` / `"FAILURE"`.
- `status` on `http_api_client_request_seconds`: HTTP status code rendered as
  a string (e.g. `"200"`), or `"0"` when the transport layer fails before
  receiving a status.
- `uri`: the API-relative path template without the server URL's path prefix
  (e.g. `/tasks/poll/batch/{taskType}`, not `/api/tasks/poll/batch/my_worker`).
  Dynamic path segments retain their `{placeholder}` tokens so that metric
  label cardinality is bounded. See the
  [path template note](#uri-label--path-templates) below.
- `exception`: the unqualified `ConductorError` variant name
  (`Http`, `Json`, `Auth`, `Server`, …), the short type name for non-
  `ConductorError` errors, or `"Panic"` for uncaught panics.

### `uri` label — path templates

The `uri` label on `http_api_client_request_seconds` carries the **path
template** (e.g. `/tasks/poll/batch/{taskType}`) rather than the
fully-resolved request path. The server URL's path prefix (e.g. `/api`) is
never included. This keeps metric cardinality bounded regardless of how many
unique workflow IDs, task types, or other dynamic path segments flow through
the SDK.

All Conductor SDKs (Go, Java, Python, Ruby, Rust) now follow this convention.
See [Detailed Technical Notes](#detailed-technical-notes) at the end of this
document for per-SDK implementation details.

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
# HTTP API client request latency (uri is the path template, not the resolved path)
http_api_client_request_seconds_bucket{method="GET",uri="/tasks/poll/batch/{taskType}",status="200",le="0.1"} 97
http_api_client_request_seconds_bucket{method="GET",uri="/tasks/poll/batch/{taskType}",status="200",le="+Inf"} 100
http_api_client_request_seconds_count{method="GET",uri="/tasks/poll/batch/{taskType}",status="200"} 100
http_api_client_request_seconds_sum{method="GET",uri="/tasks/poll/batch/{taskType}",status="200"} 8.21

# Task poll
task_poll_total{taskType="my_worker"} 124

# Task execute time (SUCCESS)
task_execute_time_seconds_bucket{taskType="my_worker",status="SUCCESS",le="0.25"} 42
task_execute_time_seconds_count{taskType="my_worker",status="SUCCESS"} 42

# Workflow start error
workflow_start_error_total{workflowType="my_wf",exception="Server"} 2
```

## Troubleshooting

### Metrics are empty

- Verify that `TaskHandler::enable_metrics` is called before `handler.start()`.
- Verify workers have polled or executed at least one task. Metrics are created
  lazily when the corresponding event occurs, so a freshly started worker with
  no traffic will have no series.
- Confirm the scrape endpoint is reachable at the expected host and port
  (default: `http://localhost:9991/metrics`).

### Missing HTTP or workflow metrics

- `http_api_client_request_seconds` is recorded by the `HttpMetricsObserver`
  installed on `ApiClient` by `enable_metrics`. If `enable_metrics` is not
  called, no HTTP metrics are emitted.
- `workflow_start_error_total` and `workflow_input_size_bytes` require the
  `WorkflowClient` to be obtained via `handler.conductor_client()` so that
  events flow through the shared `EventDispatcher`. A standalone
  `ConductorClient` created separately from the handler will not emit these
  metrics.

### High cardinality

- The `uri` label on `http_api_client_request_seconds` uses path templates
  (e.g. `/workflow/{workflowId}`) to keep cardinality bounded. If you see
  fully-resolved paths in your metrics, verify that HTTP requests are going
  through the SDK's `ApiClient` rather than a standalone HTTP client.
- Avoid embedding user identifiers or unbounded values in task type, workflow
  type, or external payload labels.

### No legacy/canonical gating

Unlike the Python, Go, Java, JavaScript, and Ruby SDKs, the Rust SDK has no
released legacy metrics surface. It ships the canonical catalog directly with
no `WORKER_CANONICAL_METRICS` environment variable and no factory/switchout
pattern. If you operate a mixed fleet of Conductor workers across multiple
SDKs, the other SDKs require `WORKER_CANONICAL_METRICS=true` to emit the
same metric names and shapes that the Rust SDK emits by default.

---

## Detailed Technical Notes

### Path template `uri` label — cross-SDK implementation

All Conductor SDKs preserve the API resource path template before path
parameter substitution and use it as the `uri` label on
`http_api_client_request_seconds`. This prevents cardinality explosion from
dynamic path segments (UUIDs, task type names, etc.) and excludes the server
URL's base path prefix.

Each SDK implements this using the mechanism most natural to its HTTP stack:

| SDK | Mechanism | Where template is captured | Where template is consumed |
|---|---|---|---|
| **Go** | `context.WithValue` with `pathTemplateKey` / `rawPathKey` | Each API resource method calls `metrics.WithPathTemplate(ctx, template)` before building the resolved URL. `executeCall` sets `WithRawPath` as fallback. | `metricsRoundTripper.RoundTrip` reads template from context; prefers template > rawPath > URL path. |
| **Java** | OkHttp `Request.tag(PathTemplateTag.class)` | `ConductorClient.buildRequest()` saves the un-substituted path as a `PathTemplateTag` on the request before replacing path params. | `ApiClientMetricsInterceptor` reads the tag at response time; falls back to `request.url().encodedPath()`. |
| **Python** | `metric_uri` keyword argument | `api_client.__call_api_no_retry()` saves `resource_path` before substitution and passes it as `metric_uri` through the call chain. | `CanonicalMetricsCollector.record_api_request_time()` prefers `metric_uri` over the resolved `uri`. |
| **Ruby** | `metric_uri` keyword argument | `ApiClient#call_api_no_retry` saves `resource_path` before substitution and passes it as `metric_uri:` to `RestClient#request`. | `RestClient#emit_http_event` uses `metric_uri` when present; falls back to `URI.parse(url).request_uri`. |
| **Rust** | `ApiPath` struct with `impl Into<ApiPath<'_>>` on public `ApiClient` methods | Static paths pass a plain `&str` (the `From<&str>` impl uses the same string for both path and metric label). Dynamic paths use `ApiPath::templated(&path, "/template/{id}")`. | `ApiClient::record_request` passes `metric_uri` directly to `HttpMetricsObserver::observe` as the `uri` label. |

In all cases the template string is the API-relative resource path (e.g.
`/workflow/{workflowId}`), never the fully-qualified URL or the base-path-
prefixed path. This means:

- `/workflow/{workflowId}` rather than `/api/workflow/abc-123-def`
- `/tasks/poll/batch/{taskType}` rather than `/tasks/poll/batch/my_worker`

Endpoints without path parameters (e.g. `/tasks/search`) use the raw resource
path directly, which is already a stable template.
