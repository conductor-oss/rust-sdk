# Run your first durable Conductor agent

**Prerequisites:** a reachable Conductor server and an LLM provider integration configured on
that server.

## 1. Configure the client

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```

For authenticated servers, also set `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET`. Do not
put provider secrets in application source; configure the provider integration on the server.

## 2. Run an example

```shell
cargo run --example sdk_playback_01_basic_agent --features agents
```

Expected result: a printed `status` (`COMPLETED`) and the model's `output`. If the request
can't reach the server, check `CONDUCTOR_SERVER_URL`. If the agent can't call a model, check
the server-side provider integration.

## 3. Create the same agent

```rust
use conductor::agents::{AgentDef, AgentRuntime};
use conductor::configuration::Configuration;
use conductor::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("greeter")?
        .with_model("openai/gpt-4o-mini")
        .with_instructions("You are a friendly assistant. Keep responses brief.");

    let result = runtime.run(&agent, "Say hello.".into()).await?;
    println!("{}", result.output);
    Ok(())
}
```

`AgentDef::new` validates `name` against `^[a-zA-Z_][a-zA-Z0-9_-]*$` up front, since it doubles
as the compiled workflow's name. `model` is a `"provider/model"` string resolved against a
Conductor integration server-side — the SDK never sees the provider's API key.

## Next steps

Use [tools](concepts/tools.md) for capabilities, [multi-agent](concepts/multi-agent.md) for
composition, and [runtime modes](concepts/deploy-serve-run.md) for deployment.
