# Agent examples

Every example in this list is a standalone Rust program that imports public `conductor` SDK
types. They accept a configured `provider/model` as their first command-line argument. Nothing
in an example silently selects a model.

Run an example from the repository after setting `CONDUCTOR_SERVER_URL` to your server's `/api`
endpoint:

```shell
cargo run --example agent_quickstart --features agents -- openai/gpt-4o-mini
```

Once the 0.1.1 crate is published, copy an example's source into `src/main.rs` of a Cargo
application with the [agent dependencies](../../README.md#install-the-sdk), then use
`cargo run -- provider/model`. The Rust closures in the tool examples return sample application
data; replace their bodies with your own service calls while keeping the SDK calls shown.

| Example | What it demonstrates | Extra setup |
|---|---|---|
| [Quickstart](../../examples/agent_quickstart.rs) | Define and run a basic agent | Server-side model integration |
| [Agent configuration](../../examples/agent_config_example.rs) | Serialize a coordinator and tool to JSON | No server connection; run with `cargo run --example agent_config_example --features agents -- provider/model` |
| [Simple tools](../../examples/agent_demo_02a_simple_tools.rs) | Local Rust function tools | None |
| [Data tools](../../examples/agent_demo_02c_data_tools.rs) | Multiple local data tools | None |
| [HTTP and MCP tools](../../examples/agent_demo_04_http_and_mcp_tools.rs) | Server-side HTTP/MCP tools plus a local formatter | Endpoints and server-side credentials below |
| [Handoffs](../../examples/agent_demo_05_handoffs.rs) | Route between specialist agents | None |
| [Sequential pipeline](../../examples/agent_demo_06_sequential_pipeline.rs) | Run agents in sequence | None |
| [Parallel agents](../../examples/agent_demo_07_parallel_agents.rs) | Run agents in parallel | None |
| [Human approval](../../examples/agent_demo_09_human_in_the_loop.rs) | Approve a tool call from the terminal | Answer the approval prompt |
| [Approval workflow](../../examples/agent_demo_09c_approval_workflow.rs) | Approve a service operation from the terminal | Answer the approval prompt |
| [Guardrails](../../examples/agent_demo_10_guardrails.rs) | Function guardrail on tool output | None |
| [Hierarchical agents](../../examples/agent_demo_13_hierarchical_agents.rs) | Delegate through nested agents | None |
| [Credential-backed HTTP tool](../../examples/agent_demo_16e_credentials_http_tool.rs) | Resolve a server-side credential for GitHub API | Configure `GITHUB_TOKEN` on the server |
| [Swarm orchestration](../../examples/agent_demo_17_swarm_orchestration.rs) | Transfer between agents | None |
| [Regex guardrails](../../examples/agent_demo_21_regex_guardrails.rs) | Filter sensitive output | None |
| [LLM guardrails](../../examples/agent_demo_22_llm_guardrails.rs) | Review output with a model | None |
| [Local tool handlers](../../examples/agent_demo_33_local_tool_handlers.rs) | Implement application tools in Rust | None |
| [Swarm with tools](../../examples/agent_demo_64_swarm_with_tools.rs) | Combine transfers and local tools | None |
| [Handoff to parallel agents](../../examples/agent_demo_66_handoff_to_parallel.rs) | Delegate to a parallel group | None |
| [Plan and compile](../../examples/agent_demo_103_plan_and_compile.rs) | Build and execute a plan with local tools | None |

## HTTP and MCP example

The HTTP/MCP example has no bundled endpoint. Point it at services you control and configure
`HTTP_TEST_API_KEY` and `MCP_TEST_API_KEY` (or your chosen names) in the Conductor server's
credential store. Pass the credential names, never the secret values, to the example:

```shell
export CONDUCTOR_EXAMPLE_HTTP_REVERSE_URL=https://your-service.example/api/string/reverse
export CONDUCTOR_EXAMPLE_MCP_URL=https://your-service.example/mcp
export CONDUCTOR_EXAMPLE_HTTP_CREDENTIAL=HTTP_TEST_API_KEY
export CONDUCTOR_EXAMPLE_MCP_CREDENTIAL=MCP_TEST_API_KEY
cargo run --example agent_demo_04_http_and_mcp_tools --features agents -- openai/gpt-4o-mini
```

The HTTP endpoint must accept a `POST` that reverses the supplied `text`. The MCP endpoint
must provide the math tool used by the example. CI supplies local fixture endpoints and
credential names explicitly.

## Human approval examples

The approval examples prompt in the terminal when an agent requests a protected tool call.
CI passes `--approve` after the model to reproduce its recorded approval; a person running the
example can review the prompt and answer `y` or `n`.
