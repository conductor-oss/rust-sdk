# Metrics and Logging

Use the [`tracing`](https://docs.rs/tracing) ecosystem for logging and the built-in
`MetricsCollector` for Prometheus metrics — this crate follows Rust ecosystem conventions here
rather than a `CONDUCTOR_LOG_LEVEL`-style custom variable.

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

```shell
RUST_LOG=conductor=debug cargo run --example hello_world
```

See [METRICS.md](METRICS.md) for the full canonical metric catalog (poll/execution/update
counters and histograms, matching the cross-SDK metrics harmonization spec) and how to wire up
`MetricsCollector`.

## Inspecting agent executions

For agent runs, inspect the workflow record itself (`WorkflowClient::get_workflow`) for inputs,
outputs, tool calls, retries, and status — there is no separate agent-specific trace store; an
agent execution is a Conductor workflow like any other.

Avoid logging credentials or unredacted sensitive data — `Credentials`'s `Debug`/`Display` impls
intentionally show only credential *names*, never values (see
[docs/agents/README.md](docs/agents/README.md)).

## Related Documentation

- **[METRICS.md](METRICS.md)**
- **[DEBUGGING.md](DEBUGGING.md)**
- **[TESTING.md](TESTING.md)** — `RUST_LOG` usage while running tests
