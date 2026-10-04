# Run your first durable Conductor agent

**Prerequisites:** a reachable Conductor server and an LLM provider integration configured on
that server.

## 1. Configure the client

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```

For authenticated servers, also set `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET`. Pass a
`provider/model` configured on the server as the example's command-line argument. CI explicitly
passes `mock/mockLLM` for recording/playback tests. Configure provider credentials on the server.

## 2. Run an example

```shell
cargo run --example agent_quickstart --features agents -- openai/gpt-4o-mini
```

For an already-running local Conductor on port 8080 without SDK authentication, use
[`scripts/validate-local-agent.sh`](../../scripts/validate-local-agent.sh) from the repository
root. It checks server health and model availability and fails unless the agent completes with
output. Expected result: a printed `status` (`COMPLETED`), the model's `output`, and an
`execution_id` for inspection in the UI. If the request
can't reach the server, check `CONDUCTOR_SERVER_URL`. If the agent can't call a model, check
the server-side provider integration.

## 3. Create the same agent

The [README quickstart](../../README.md#agent-quickstart) gives the complete `Cargo.toml`
dependencies, `src/main.rs`, and `cargo run -- provider/model` command for an application using
the 0.1.1 SDK once published. The checked-in [agent quickstart source](../../examples/agent_quickstart.rs)
is the same application code and is what CI runs with `mock/mockLLM`.

`AgentDef::new` validates `name` against `^[a-zA-Z_][a-zA-Z0-9_-]*$` up front, since it doubles
as the compiled workflow's name. `model` is a `"provider/model"` string resolved against a
Conductor integration server-side — the SDK never sees the provider's API key.

## Next steps

Use [tools](concepts/tools.md) for capabilities, [multi-agent](concepts/multi-agent.md) for
composition, and [runtime modes](concepts/deploy-serve-run.md) for deployment.
