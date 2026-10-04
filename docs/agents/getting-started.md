# Run your first durable Conductor agent

Requires a Conductor server with a configured model.

## 1. Configure the client

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```

For authenticated servers, also set `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET`.
Configure provider credentials on the server, then pass its `provider/model` name below.

## 2. Run an example

```shell
cargo run --example agent_quickstart --features agents -- openai/gpt-4o-mini
```

For an unauthenticated local server on port 8080, run
[`scripts/validate-local-agent.sh`](../../scripts/validate-local-agent.sh) from the repo root.
It checks server health, model availability, and the agent result. The example prints
`status`, `output`, and `execution_id`.

## 3. Create the same agent

The [README quickstart](../../README.md#agent-quickstart) has the dependencies and application
code. CI runs the [checked-in example](../../examples/agent_quickstart.rs) with `mock/mockLLM`.

`AgentDef::new` validates the agent name (`^[a-zA-Z_][a-zA-Z0-9_-]*$`). Conductor resolves
`provider/model` on the server; the SDK does not receive the provider API key.

## Next steps

See [tools](concepts/tools.md), [multi-agent](concepts/multi-agent.md), and
[runtime modes](concepts/deploy-serve-run.md).
