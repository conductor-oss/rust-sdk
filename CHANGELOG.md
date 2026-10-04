# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Durable agents (`agents` feature): tools, multi-agent runs, approval, guardrails, scheduling, and streaming
- Standalone agent examples with explicit `provider/model` selection; CI uses `mock/mockLLM`
- Agent quickstart and local validation script

### Changed

- Secret integration tests now check OSS reads and its `501` response for writes.
- **Breaking in 0.1.2:** Server response fixes change these public types and methods:
  - `EventClient::get_all_queue_configurations` now returns `HashMap<String, String>`.
  - `CreatedAccessKey` no longer has `status`.
  - `AuthorizationClient::get_granted_permissions_for_{user,group}` unwraps `grantedAccess`; `GrantedPermission` adds `tag`.
  - `WorkflowSchedule` moves workflow name and version under `startWorkflowRequest`, renames `update_time` to `updated_time`, and adds `paused_reason` and `description`.

### Fixed

- `SecretClient::get_secret` reads the plain-text response.
- `SecretClient::list_all_secret_names` uses `POST /secrets`; `list_secrets_that_user_can_grant_access_to` uses `GET` without `grantable=true`.
- `SchedulerClient::pause_schedule` and `resume_schedule` try `PUT`, then `GET` on `405` for older Orkes servers.

## [0.1.0] - 2026-06-29

First published release (`conductor-sdk` on crates.io).

### Added

- Canonical metrics: harmonized metric surface aligned with the cross-SDK catalog -- see [METRICS.md](METRICS.md) for the full catalog, configuration, and implementation details
- Bounded `uri` label on `http_api_client_request_seconds`: uses path templates (e.g. `/workflow/{workflowId}`) instead of fully-resolved paths, preventing metric cardinality explosion from dynamic IDs
- `WorkflowStatusProbe` in harness: opt-in probe (via `HARNESS_PROBE_RATE_PER_SEC`) that exercises UUID-bearing endpoints to validate template URI metrics
- Worker panic resilience: spawned task executions are wrapped in `catch_unwind` so that an uncaught panic is logged, publishes a `thread_uncaught_exceptions_total` metric event, and cleans up tracking state (semaphore permit, active task count) instead of silently leaking resources

### Changed

- The Rust SDK is unreleased, so the emitted metric surface is canonical on day one; there is no legacy mode or migration path
- `ApiClient` public methods accept `impl Into<ApiPath>` to pair resolved paths with bounded-cardinality metric templates -- see [METRICS.md](METRICS.md#detailed-technical-notes)
