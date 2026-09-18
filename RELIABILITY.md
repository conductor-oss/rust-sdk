# Reliability: Timeouts, Retries, Idempotency, and Domains

Set poll, response, and execution timeouts on every task definition (`TaskDef::timeout_seconds`,
`response_timeout_seconds`, `poll_timeout_seconds`) rather than relying on defaults for anything
business-critical. For a task whose real execution time can approach its response timeout, see
[LEASE_EXTENSION.md](LEASE_EXTENSION.md) instead of just widening the timeout.

Retry only idempotent or already-compensated work; use exponential backoff for transient remote
failures (the pattern `TaskClient::update_task_with_retry` already uses internally for task
updates). Route resource-bound work to a `domain` (see
[WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)) to isolate it from unrelated workers.

## Workers may receive a task more than once

A worker can be handed the same task again after a lease expires (slow execution, crash, network
partition) even if the first attempt actually completed its side effect. Persist an idempotency
key **before** an external side effect (payment, email, write to another system) and check it on
each attempt, rather than assuming exactly-once delivery.

For work that's expected to run long, use either:
- `WorkerOutput::InProgress(TaskInProgress::new(seconds))` to explicitly re-queue and resume
  later (chunked, resumable work), or
- automatic lease-extension heartbeats (`WorkerConfig::with_lease_extend_enabled`) to keep a
  single long `execute()` call's lease alive without chunking — see
  [LEASE_EXTENSION.md](LEASE_EXTENSION.md).

Verify retry/idempotency behavior with failure-path tests (see [WORKFLOW_TESTING.md](WORKFLOW_TESTING.md)'s
`TaskMock` retry-sequence support), and inspect `reason_for_incompletion` on a failed task before
blindly retrying it.

## Related Documentation

- **[LEASE_EXTENSION.md](LEASE_EXTENSION.md)**
- **[WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)** — `TaskInProgress`, domains, thread count
- **[WORKFLOW_TESTING.md](WORKFLOW_TESTING.md)** — simulating retries/timeouts with `TaskMock`
