# Core Workflow and Worker Quickstart

**Prerequisites:** Rust 1.85+, and a reachable Conductor server from
[SERVER_SETUP.md](SERVER_SETUP.md).

Run the maintained example:

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
cargo run --example hello_world
```

Expected result: the example registers a workflow definition and a worker, starts the workflow,
polls/executes the worker, and prints the workflow's final output. If the task stays `SCHEDULED`,
verify that a worker is polling the exact task type (see [DEBUGGING.md](DEBUGGING.md)).

Continue with [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md) for worker tuning, or
[API_MAP.md](API_MAP.md) for where to go next for a given capability.

## Related Documentation

- **[examples/hello_world.rs](examples/hello_world.rs)**
- **[README.md](README.md)** — full crate overview
