# Deployment, Scaling, and Graceful Shutdown

Run `TaskHandler`/`WorkerHost` (and `AgentRuntime::serve()` for agents) as long-lived services,
not per-request objects. Scale by adding worker instances and use `domain`/`thread_count` to
isolate and size workloads (see [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)).

```rust
let mut handler = TaskHandler::new(config.clone())?;
handler.add_worker(my_worker);
handler.start().await?;

// ... process runs for the service's lifetime ...

// On shutdown:
handler.stop().await?;
```

Do not construct a new `TaskHandler`/`ConductorClient`/`AgentRuntime` per web request or per
task — reuse clients for the application lifetime; each holds its own connection pool.

## Graceful shutdown

Call `stop()` (or `WorkerHost::stop()`/`WorkerHost::wait_for_shutdown()` for the higher-level
wrapper) on process shutdown so in-flight tasks finish naturally instead of being abandoned —
an abandoned task's lease expires server-side and gets redelivered to another worker, which is
safe but wastes the work already done. `stop()` signals pollers to stop taking new tasks and
waits (up to a timeout) for in-flight tasks to complete before returning.

See [RELIABILITY.md](RELIABILITY.md) for timeout/retry policy and
[LEASE_EXTENSION.md](LEASE_EXTENSION.md) for keeping a lease alive during a slow shutdown drain.

## Related Documentation

- **[src/worker/task_handler.rs](src/worker/task_handler.rs)**
- **[src/worker/worker_host.rs](src/worker/worker_host.rs)**
- **[WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md)**
