# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0]

### Added

- Canonical metrics: harmonized metric surface aligned with the cross-SDK catalog -- see [METRICS.md](METRICS.md) for the full catalog, configuration, and implementation details
- Bounded `uri` label on `http_api_client_request_seconds`: uses path templates (e.g. `/workflow/{workflowId}`) instead of fully-resolved paths, preventing metric cardinality explosion from dynamic IDs
- `WorkflowStatusProbe` in harness: opt-in probe (via `HARNESS_PROBE_RATE_PER_SEC`) that exercises UUID-bearing endpoints to validate template URI metrics
- Worker panic resilience: spawned task executions are wrapped in `catch_unwind` so that an uncaught panic is logged, publishes a `thread_uncaught_exceptions_total` metric event, and cleans up tracking state (semaphore permit, active task count) instead of silently leaking resources

### Changed

- The Rust SDK is unreleased, so the emitted metric surface is canonical on day one; there is no legacy mode or migration path
- `ApiClient` public methods accept `impl Into<ApiPath>` to pair resolved paths with bounded-cardinality metric templates -- see [METRICS.md](METRICS.md#detailed-technical-notes)
