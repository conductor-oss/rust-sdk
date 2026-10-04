# Conductor Agents — Rust SDK

Conductor agents run as workflows. Tool calls are tasks, and runs survive process restarts.

## Install

```toml
[dependencies]
conductor = { package = "conductor-sdk", version = "0.1.2", features = ["agents"] }
tokio = { version = "1", features = ["full"] }
```

The agent APIs are planned for 0.1.2. Configure a model on a reachable Conductor server and
pass its `provider/model` name as the first argument to each example.

## Start here

- [Getting started](getting-started.md) — configure a server and run a basic agent.
- [Agent examples](examples.md) — runnable SDK programs and their prerequisites.
- [Deploy · Serve · Run](concepts/deploy-serve-run.md) — choose a runtime mode.
- [Scheduling](concepts/scheduling.md) — attach cron schedules to a deployed agent.

## Build agents

- [Agents](concepts/agents.md), [tools](concepts/tools.md), and [multi-agent](concepts/multi-agent.md)
- [Guardrails](concepts/guardrails.md) and [termination](concepts/termination.md)
- [Callbacks](concepts/callbacks.md) and [stateful agents](concepts/stateful.md)
- [Streaming and human-in-the-loop](concepts/streaming-hitl.md)

## Operate and inspect

- [Runtime reference](reference/runtime.md) and [control-plane reference](reference/client.md)
- [Agent-definition fields](reference/agent-definition.md)
