# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Integration tests: `orkes_client_tests.rs`'s secret read tests (`get`/`list`/`exists`) now assert for real against OSS Conductor, using a dummy env-backed secret seeded in `scripts/docker-compose-oss.yaml`; secret writes (`put`/`delete`) are asserted to fail with a real `501` on OSS (read-only backend) instead of being skipped
- **Breaking.** Response shapes corrected against both server families. `conductor-sdk 0.1.0` is published, and Cargo treats the minor version as the compatibility axis below 1.0 (`0.1` resolves to `>=0.1.0, <0.2.0`), so these must ship as `0.2.0` -- released as `0.1.1` they would break existing dependents on `cargo update`:
  - `EventClient::get_all_queue_configurations` returns `HashMap<String, String>` instead of `Vec<QueueConfiguration>`, matching Orkes' `Map<String, String> getQueueNames()`
  - `CreatedAccessKey` no longer has a `status` field -- the server's `CreateAccessKeyResponse` is `{id, secret}`; `status` exists only on the `AccessKeyResponse` returned by the list/toggle endpoints
  - `AuthorizationClient::get_granted_permissions_for_{user,group}` unwrap the server's `{"grantedAccess": [...]}` envelope, and `GrantedPermission` gained the `tag` field the server sends
  - `WorkflowSchedule` dropped its top-level `workflow_name` and `workflow_version`; neither server family has such a field, both nest the values under `startWorkflowRequest`. Its `update_time` is renamed `updated_time` (the server sends `updatedTime`), and `paused_reason`/`description` were added

### Fixed

- `SecretClient::get_secret` reads the response as raw text rather than parsing it as JSON. `GET /secrets/{key}` is declared `produces = MediaType.TEXT_PLAIN_VALUE` on both OSS (`SecretController`) and Orkes (`SecretResource`)
- `SecretClient::list_all_secret_names` sends `POST /secrets`, not `GET`. The two verbs are different operations on both server families: `POST` lists every secret name, `GET` lists only the names the caller has access to. They agree on OSS, which has no RBAC, so the wrong verb went unnoticed there while silently returning an access-filtered subset against Orkes. `list_secrets_that_user_can_grant_access_to` keeps `GET` and drops the `grantable=true` parameter, which no server reads
- `SchedulerClient::pause_schedule`/`resume_schedule` send `PUT` first and fall back to `GET` on a `405`. OSS Conductor maps these as `@PutMapping` only; Orkes Conductor accepts both verbs as of the dual `@RequestMapping(method = {GET, PUT})` added in 2026-07, and is `GET`-only in deployments older than that. `pause_all_schedules`, `resume_all_schedules` and `requeue_all_execution_records` remain `GET`, which is how both families map those admin endpoints -- `tests/scheduler_verb_fallback_tests.rs` pins the whole contract

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
