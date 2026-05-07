# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Metrics harmonization** - canonical metric surface aligned with the cross-SDK catalog (no `WORKER_CANONICAL_METRICS` env var; the Rust SDK is unreleased so the emitted surface is canonical on day one)
  - `MetricsCollector` now emits the harmonized cross-SDK catalog: counters (`task_poll_total`, `task_poll_error_total`, `task_execution_started_total`, `task_execute_error_total`, `task_update_error_total`, `task_paused_total`, `task_ack_error_total`, `task_ack_failed_total`, `task_execution_queue_full_total`, `external_payload_used_total`, `thread_uncaught_exceptions_total`, `workflow_start_error_total`), histograms (`task_poll_time_seconds`, `task_execute_time_seconds`, `task_update_time_seconds`, `http_api_client_request_seconds`, `task_result_size_bytes`, `workflow_input_size_bytes`), and `active_workers{taskType}` gauge. Time buckets `0.001…10s`; size buckets `100…10_000_000` bytes; labels are camelCase.
  - `HttpMetricsObserver` trait and `NoopHttpMetricsObserver`. `MetricsCollector` implements `HttpMetricsObserver`; `TaskHandler::enable_metrics` automatically installs it on the underlying `ApiClient`. Transport failures record `status="0"` to match the cross-SDK convention.
  - `events::exception::exception_label(&ConductorError)` produces bounded-cardinality `&'static str` labels (`"Http"`, `"Json"`, `"Server"`, etc.) used everywhere the canonical `exception` label is emitted.
  - New event types in `events::task_runner_events`: `PollSkippedPaused`, `TaskUpdateCompleted`, `ThreadUncaughtException`, `WorkflowStarted`, `WorkflowStartFailure`. `WorkflowClient` and `ConductorClient` emit workflow events through the dispatcher.

### Changed

- **Metrics harmonization** - label renames; no legacy mode (other Conductor SDKs that did release metrics — Python, Go, Java, JavaScript, Ruby — ship a gated switch via `WORKER_CANONICAL_METRICS`; Rust skips the gate)
  - Metric labels renamed to camelCase (`task_type → taskType`, `error_type → exception`, plus `version`, `method`, `uri`, `status`, `entityName`, `operation`, `payloadType`). The pre-harmonization metrics that existed on `main` (snake_case labels, `conductor_*` prefix, mismatched buckets) are not preserved.
  - Default `MetricsSettings::namespace` is now `""` (was implicitly `"conductor"`) to align with canonical naming.
  - New top-level `METRICS.md` with the canonical catalog, bucket sets, label conventions, configuration via `TaskHandler::enable_metrics`, an "Intentional divergences" table for Rust-specific omissions (`task_ack_*`, `task_execution_queue_full_total`, `external_payload_used_total` are registered but not incremented; `worker_restart_total` and `task_execution_completed_total` are not emitted), and an explicit "No legacy/canonical gating" section.
  - `DESIGN.md`, `docs/WORKER.md`, and `WORKER_COMPARISON.md` point to `METRICS.md`.
