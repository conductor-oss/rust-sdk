# Agents

**Audience:** developers authoring durable, LLM-backed Conductor agents.

## Prerequisites

A reachable Conductor server with the target model's provider integration configured. Keep
provider credentials on the server, never in application source.

## Define an agent

Build an [`AgentDef`](../reference/agent-definition.md) with a stable name, a `"provider/model"`
string, instructions, and optional tools or sub-agents:

```rust
use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(JsonSchema, Deserialize)]
struct GetWeatherArgs { city: String }

let get_weather = ToolDef::function::<GetWeatherArgs, _, _>(
    "get_weather",
    "Get current weather for a city",
    conductor::schema::generate_schema::<GetWeatherArgs>(true),
    |args: GetWeatherArgs| async move { Ok(serde_json::json!({ "temp_f": 72 })) },
);

let agent = AgentDef::new("weather_assistant")?
    .with_model("openai/gpt-4o")
    .with_instructions("You are a helpful weather assistant. Use the get_weather tool.")
    .with_tool(get_weather)
    .with_max_turns(10)?;

let runtime = AgentRuntime::new(Configuration::from_env())?;
let result = runtime.run(&agent, "What's the weather in Austin?".into()).await?;
println!("{}", result.output);
```

## Instructions and runtime overrides

`instructions` is a plain string set via `with_instructions`. Use [`RunSettings`](deploy-serve-run.md)
to override model/temperature/max-tokens/reasoning-effort for one run without mutating a shared
`AgentDef`. `model` may be left unset only for a `Router`/`Sequential`/`Parallel` parent that
inherits a model from a sub-agent, or an external/framework-marker agent — see
[multi-agent](multi-agent.md).

## Expected result

`runtime.run(...)` compiles the agent to a Conductor workflow, starts required local tool
workers, and blocks to an [`AgentResult`](../reference/runtime.md). The Conductor UI shows the
durable execution and every tool call it made.

## Common failures

- A model error usually means the provider integration or model is missing on the **server**,
  not the Rust process.
- A name that doesn't match `^[a-zA-Z_][a-zA-Z0-9_-]*$` is rejected by `AgentDef::new`.
- `with_strategy(Strategy::Router)` / `Strategy::PlanExecute` fail if the required
  `with_router`/`with_planner` call didn't happen first in the builder chain — see
  [multi-agent](multi-agent.md).

## Next steps

Use [tools](tools.md) for capabilities, [multi-agent](multi-agent.md) for composition, and
[runtime modes](deploy-serve-run.md) for deployment.
