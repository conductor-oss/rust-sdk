# Agent examples

Pass a server-configured `provider/model` as the first argument to each example.

Run the [simple tools example](../../examples/agent_demo_02a_simple_tools.rs) against a local
server with `openai/gpt-4o-mini` configured:

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
cargo run --example agent_demo_02a_simple_tools --features agents -- openai/gpt-4o-mini
```

Use another configured model if needed. For an agent without tools, see
[`agent_quickstart`](../../examples/agent_quickstart.rs).

After 0.1.1 is published, you can use the simple tools source in a new project with:

```toml
[dependencies]
conductor = { package = "conductor-sdk", version = "0.1.1", features = ["agents"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Run it with `cargo run -- openai/gpt-4o-mini`. Replace the sample tool data with your own.

| Example | What it demonstrates | Extra setup |
|---|---|---|
| [Quickstart](../../examples/agent_quickstart.rs) | Basic agent | None |
| [Agent configuration](../../examples/agent_config_example.rs) | Serialize an agent and tool | No server |
| [Simple tools](../../examples/agent_demo_02a_simple_tools.rs) | Local Rust function tools | None |
| [Data tools](../../examples/agent_demo_02c_data_tools.rs) | Multiple local data tools | None |
| [HTTP and MCP tools](../../examples/agent_demo_04_http_and_mcp_tools.rs) | HTTP, MCP, and local tools | Endpoints and credentials below |
| [Handoffs](../../examples/agent_demo_05_handoffs.rs) | Route between specialist agents | None |
| [Sequential pipeline](../../examples/agent_demo_06_sequential_pipeline.rs) | Run agents in sequence | None |
| [Parallel agents](../../examples/agent_demo_07_parallel_agents.rs) | Run agents in parallel | None |
| [Human approval](../../examples/agent_demo_09_human_in_the_loop.rs) | Approve a tool call from the terminal | Answer the approval prompt |
| [Approval workflow](../../examples/agent_demo_09c_approval_workflow.rs) | Approve a service operation from the terminal | Answer the approval prompt |
| [Guardrails](../../examples/agent_demo_10_guardrails.rs) | Function guardrail on tool output | None |
| [Hierarchical agents](../../examples/agent_demo_13_hierarchical_agents.rs) | Delegate through nested agents | None |
| [Credential-backed HTTP tool](../../examples/agent_demo_16e_credentials_http_tool.rs) | GitHub API credential | Configure `GITHUB_TOKEN` on the server |
| [Swarm orchestration](../../examples/agent_demo_17_swarm_orchestration.rs) | Transfer between agents | None |
| [Regex guardrails](../../examples/agent_demo_21_regex_guardrails.rs) | Filter sensitive output | None |
| [LLM guardrails](../../examples/agent_demo_22_llm_guardrails.rs) | Review output with a model | None |
| [Local tool handlers](../../examples/agent_demo_33_local_tool_handlers.rs) | Implement application tools in Rust | None |
| [Swarm with tools](../../examples/agent_demo_64_swarm_with_tools.rs) | Combine transfers and local tools | None |
| [Handoff to parallel agents](../../examples/agent_demo_66_handoff_to_parallel.rs) | Delegate to a parallel group | None |
| [Plan and compile](../../examples/agent_demo_103_plan_and_compile.rs) | Build and execute a plan with local tools | None |

## HTTP and MCP example

Set service endpoints and store `HTTP_TEST_API_KEY` and `MCP_TEST_API_KEY` on the Conductor
server. Pass credential names to the example:

```shell
export CONDUCTOR_EXAMPLE_HTTP_REVERSE_URL=https://your-service.example/api/string/reverse
export CONDUCTOR_EXAMPLE_MCP_URL=https://your-service.example/mcp
export CONDUCTOR_EXAMPLE_HTTP_CREDENTIAL=HTTP_TEST_API_KEY
export CONDUCTOR_EXAMPLE_MCP_CREDENTIAL=MCP_TEST_API_KEY
cargo run --example agent_demo_04_http_and_mcp_tools --features agents -- openai/gpt-4o-mini
```

The HTTP endpoint must reverse `text` on `POST`; the MCP endpoint must provide the math tool.

## Human approval examples

Answer `y` or `n` at the terminal prompt. CI passes `--approve` after the model.
