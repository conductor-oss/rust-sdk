# Conductor Agents — Rust SDK

Build durable, LLM-backed agents on Conductor. Agents run as real Conductor workflows: tool
calls are ordinary tasks, execution survives process restarts, and the Conductor UI shows the
full run.

## Install

```toml
[dependencies]
conductor-sdk = { version = "...", features = ["agents"] }
```

Requirements: a reachable Conductor server with an LLM provider integration configured
server-side. Replace example model strings with a model enabled on that server.

## Start here

- [Getting started](getting-started.md) — configure a server and run a basic agent.
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

## What Conductor adds

| Capability | Conductor agent runtime |
|---|---|
| Process recovery | Durable workflow state resumes after a restart via `AgentRuntime::resume`. |
| Local tools | Tools run as ordinary Conductor worker tasks, independently scalable. |
| Long-running work | Human approval and schedules don't occupy application threads. |
| Observability | Inputs, outputs, tool calls, and status share one execution record in the UI. |
