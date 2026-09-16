# Lease Extension (Automatic Heartbeat)

Keeps a long-running task's server-side lease alive automatically by sending periodic
`extend_lease` heartbeats while a worker is still executing — so a task doesn't get treated as
timed out (and re-queued to another worker) just because it's taking a while, without you having
to manually break the work into `TaskInProgress` chunks (see [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md#long-running-tasks)
for that alternative, still-supported approach).

Ports python-sdk's `LeaseManager` design; see the "Why a spawned task, not a shared manager"
section below for the one deliberate implementation difference.

## Enabling it

Off by default — only turn it on for workers whose execution time can approach the task's
`responseTimeoutSeconds`.

```rust
use conductor::worker::FnWorker;

let worker = FnWorker::new("slow_report", |task| async move {
    // work that can take a while, close to responseTimeoutSeconds
    Ok(WorkerOutput::completed_with_result(generate_report(&task).await))
})
.with_lease_extend_enabled(true);
```

Or via environment variable, following the same [hierarchical override](WORKER_CONFIGURATION.md#configuration-hierarchy)
as every other worker property:

```bash
export CONDUCTOR_WORKER_ALL_LEASE_EXTEND_ENABLED=true
export CONDUCTOR_WORKER_SLOW_REPORT_LEASE_EXTEND_THRESHOLD=0.5
```

## Configurable properties

| Property | Type | Default | Description |
|---|---|---|---|
| `LEASE_EXTEND_ENABLED` | bool | `false` | Enable automatic lease-extension heartbeats for this worker. |
| `LEASE_EXTEND_THRESHOLD` | float | `0.8` | Fraction of the task's `responseTimeoutSeconds` after which a heartbeat is sent (and repeated at the same interval for as long as the task keeps running). |

Both are also available as `WorkerConfig`/`FnWorker` builder methods:
`with_lease_extend_enabled(bool)` / `with_lease_extend_threshold(f64)`.

## How it works

For each task execution, if lease extension is enabled and the task's `responseTimeoutSeconds`
is set:

1. `interval = responseTimeoutSeconds * lease_extend_threshold` is computed. If that's under one
   second, no heartbeat is scheduled (matching python-sdk) — the interval isn't worth the extra
   traffic.
2. A heartbeat loop starts alongside the worker's `execute()` future, waiting `interval` before
   its first tick (not immediately).
3. On each tick, it sends `TaskResult { task_id, workflow_instance_id, status: InProgress,
   extend_lease: true, .. }` via the task client — up to 3 attempts with a short backoff between
   retries if a send fails, matching python-sdk's `LEASE_EXTEND_RETRY_COUNT`. This uses a
   dedicated fast retry, not `TaskClient::update_task_with_retry`'s 10/20/30s schedule, which is
   sized for terminal completion updates rather than a fast-repeating keep-alive.
4. The loop is aborted as soon as `execute()` returns (success, failure, or panic) — no heartbeat
   is sent after the task has actually finished.

## Why a spawned task, not a shared manager

Python-sdk's `LeaseManager` is a **single process-wide background thread** shared across every
worker, because OS threads are relatively expensive there and multiprocessing/thread-pool
scaling makes a shared manager worth the coordination overhead. Rust's `tokio::spawn` is cheap
enough that spawning one heartbeat task per in-flight execution — cancelled via `JoinHandle::abort()`
when the task completes — needs no shared state, no lock, and no singleton lifecycle to manage.
Same wire behavior and timing, simpler implementation for this runtime model.

## Related Documentation

- **[WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)** — full hierarchical worker configuration
- **[src/worker/task_runner.rs](src/worker/task_runner.rs)** — `maybe_spawn_lease_heartbeat` /
  `send_lease_heartbeats`
- **[src/configuration/worker_config.rs](src/configuration/worker_config.rs)** — `WorkerConfig`
