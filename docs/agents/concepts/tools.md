# Tools

**Audience:** authors exposing local or server-native capabilities to a Conductor agent.

## Prerequisites

Decide whether a capability needs local Rust code (a function tool) or can run entirely
server-side (HTTP, MCP, RAG, media generation). Declare any credentials the tool needs by name;
never read them from process environment variables.

## Define a function tool

`ToolDef::function` derives the tool's schema from an args struct implementing
`JsonSchema + DeserializeOwned`. Each call becomes a durable, retryable Conductor task:

```rust
use conductor::agents::{Credentials, ToolContext, ToolDef};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(JsonSchema, Deserialize)]
struct CreateIssueArgs { title: String }

let create_issue = ToolDef::function_with_credentials::<CreateIssueArgs, _, _>(
    "create_issue",
    "File a GitHub issue",
    conductor::schema::generate_schema::<CreateIssueArgs>(true),
    |args: CreateIssueArgs, creds: &Credentials| async move {
        let token = creds.get("GITHUB_TOKEN")?;
        Ok(serde_json::json!({ "created": args.title }))
    },
)
.with_credentials(vec!["GITHUB_TOKEN".to_owned()]);
```

`ToolDef::function_with_context` is the same shape but receives a [`ToolContext`] instead,
for reading/writing per-execution state that persists across the agent's tool calls
(`ToolContext::get_state`/`set_state`).

## Choose the right constructor

| Need | Constructor |
|---|---|
| Local Rust logic | `ToolDef::function` / `function_with_credentials` / `function_with_context` |
| HTTP endpoint | `ToolDef::http` / `http_templated` |
| OpenAPI/Swagger/Postman discovery | `ToolDef::api` |
| MCP server | `ToolDef::mcp` |
| Human decision | `ToolDef::human` |
| Delegate to another agent | `ToolDef::agent` |
| Image / audio / video / PDF generation | `ToolDef::image` / `audio` / `video` / `pdf` |
| Vector index / search | `ToolDef::rag_index` / `rag_search` |
| Workflow Message Queue | `ToolDef::wait_for_message` |

Every constructor except `function`/`agent`/`human` compiles to a Conductor system task — no
local worker process runs it. `http`/`http_templated`/`mcp`/`api` accept `${NAME}` placeholders
in `url`/`headers`; each placeholder must also be in that tool's declared `credentials`, or
construction fails with an error naming the undeclared placeholder.

## Credentials

A tool/agent declares credential *names*, never values:

```rust
agent.with_tool_credentials("create_issue", vec!["GITHUB_TOKEN".to_owned()])?;
```

The server resolves each declared name against its own secret store and attaches resolved
values to the specific polled `Task`, never to task input. Read them inside a handler via
[`Credentials`]: `Credentials::get(name)` fails closed
(`ConductorError::CredentialNotFound`) if the name isn't present — there is no fallback to
`std::env::var()`. `Credentials`'s `Debug`/`Display` impls show only the names held, never the
values.

## Reliability and approval

Set `with_timeout_seconds`, `with_max_calls`, and `with_approval_required(true)` for
destructive operations — see [streaming and approval](streaming-hitl.md). Attach a
tool-scoped [`Guardrail`](guardrails.md) with `ToolDef::with_guardrail` independent of the
owning agent's guardrails.

## Expected result and failures

A successful tool call appears as a named task in the agent's execution. A task stuck
`SCHEDULED` has no compatible worker polling — call `AgentRuntime::serve`/`serve_tools` for
every process that owns a local tool. A missing declared credential fails the task closed
before the handler runs, rather than the handler receiving an empty value.

## Next steps

Continue with [guardrails](guardrails.md), [streaming and approval](streaming-hitl.md), or
[multi-agent](multi-agent.md).
