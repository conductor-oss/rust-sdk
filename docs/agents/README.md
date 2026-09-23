# Agents

Status: **implemented** in `src/agents/`, gated behind the `agents` Cargo feature (zero cost when
unused):

```toml
[features]
default = []
agents = []
```

An agent is a durable, LLM-driven workflow: you declare an `AgentDef` (model, instructions,
tools, sub-agents, guardrails, ...), `AgentRuntime` compiles it to an `agentConfig` and runs it
as a real Conductor workflow (`DoWhile` + `LlmChatComplete` + one task per tool/guardrail/router),
while tool execution stays local. This is purely additive — the existing
`LlmChatComplete`/`LlmTextComplete` workflow-task builders (`agentic_workflow.rs`/
`multiagent_chat.rs` examples) remain the low-level workflow DSL that this layer sits above.

Known, tracked gaps (not silently skipped — each has a reason on record):
- `AgentEvent` has no `Unknown`/catch-all variant, so a future new server event kind would error
  `AgentStream::next` rather than degrade gracefully.
- `start_agent` as an `EventHandler` action (start an agent execution in response to a
  workflow/task event) is not implemented — needs its exact wire shape confirmed against the
  server before building `EventHandlerAction::StartAgent`.
- `WorkerRestarter`-style SIGKILL + respawn-on-monitor recovery: decided N/A — this crate's
  tokio-task-per-worker model has no OS-process supervisor to restart.
- `GPTAssistantAgent` (an OpenAI Assistants-API wrapper): decided not to build — the Assistants
  API it would wrap was fully sunset by OpenAI on 2026-08-26, so every endpoint it would call
  now errors unconditionally, permanently.

## Architecture

```
AgentClient / AgentRuntime          control-plane + local tool-worker host
        │
AgentDef (+ RunSettings)            declarative definition: name, model, tools, guardrails,
        │                           sub-agents, strategy, termination, ...
        ▼
AgentConfigSerializer                AgentDef -> agentConfig (wire JSON)
```

- **`AgentClient`** — thin transport over `/agent/*` (compile / deploy / start / status /
  execution / executions / respond / stop / signal / stream), same shape as `SecretClient` /
  `PromptClient`: `#[derive(Clone)] struct { api: ApiClient }`, registered in `client/mod.rs` +
  a `ConductorClient::agent_client()` accessor. No Orkes-extension split needed — one concrete
  client is enough.
- **`AgentRuntime`** — compiles `AgentDef` → `agentConfig`, drives lifecycle
  (`plan`/`deploy`/`serve`/`run`/`start`/`resume`). Composes the existing `TaskHandler` used by
  every other worker in this SDK for local tool execution — it does **not** run its own polling
  loop, so agent tool workers get pooling (`Semaphore` + `thread_count`), panic isolation,
  retry-on-update, graceful shutdown, and Prometheus metrics identically to any other worker.
  `AgentRuntime::task_handler()` exposes the handler so callers can invoke
  `TaskHandler::verify_workers_started(timeout)` right after `serve`/`resume` — an opt-in check
  that a registered worker actually started polling.
- **`AgentHandle` / `AgentResult` / `AgentStatus` / `AgentEvent` / `AgentStream`** — result,
  streaming, and human-in-the-loop (HITL) types.

`agents/` is a top-level module (not nested under `worker/` or `client/`) because it spans both:
it needs a transport client and a worker host.

```rust
pub struct AgentRuntime {
    agent_client: AgentClient,
    task_handler: TaskHandler,
    config: AgentRuntimeConfig,
}

impl AgentRuntime {
    pub fn new(configuration: Configuration) -> Result<Self>;
    pub fn with_config(configuration: Configuration, config: AgentRuntimeConfig) -> Result<Self>;

    pub async fn plan(&self, agent: &AgentDef) -> Result<Value>;                        // compile only
    pub async fn deploy(&mut self, agent: &AgentDef) -> Result<DeploymentInfo>;          // + register + local workers, no exec
    pub async fn deploy_with_schedules(&mut self, agent: &AgentDef, schedules: Option<&[Schedule]>) -> Result<DeploymentInfo>;
    pub async fn serve(&mut self, agents: &[AgentDef]) -> Result<()>;                   // deploy + block
    pub async fn run(&mut self, agent: &AgentDef, input: Value) -> Result<AgentResult>;       // + execute, block to result
    pub async fn start(&mut self, agent: &AgentDef, input: Value) -> Result<AgentHandle>;     // + execute, don't block
    pub async fn resume(&mut self, execution_id: &str) -> Result<AgentHandle>;          // reattach after a process restart
    pub fn task_handler(&self) -> &TaskHandler;
    pub async fn shutdown(self) -> Result<()>;
}
```

`resume` reattaches to an existing execution after a restart: it registers workers the same way
`serve` does, then wraps the given `execution_id` in an `AgentHandle` — no per-execution worker
"domain" bookkeeping is needed since this crate's worker registration has no such concept.

`AgentClient`:

```rust
#[derive(Clone)]
pub struct AgentClient { api: ApiClient }

impl AgentClient {
    pub fn new(api: ApiClient) -> Self;
    pub async fn compile_agent(&self, config: &Value) -> Result<Value>;
    pub async fn deploy_agent(&self, config: &Value) -> Result<Value>;
    pub async fn start_agent(&self, config: &Value, input: Value) -> Result<Value>;
    pub async fn get_status(&self, execution_id: &str) -> Result<AgentStatus>;
    pub async fn get_execution(&self, execution_id: &str) -> Result<Value>;
    pub async fn list_executions(&self, params: &[(&str, &str)]) -> Result<Value>;
    pub async fn respond(&self, execution_id: &str, body: &Value) -> Result<()>;
    pub async fn stop(&self, execution_id: &str) -> Result<()>;
    pub async fn signal(&self, execution_id: &str, message: &str) -> Result<()>;
    pub fn stream_events(&self, execution_id: &str) -> impl Stream<Item = Result<AgentEvent>>;
}
```

`stream_events` wraps SSE over `reqwest`'s streaming body: returns `ConductorError::SseUnavailable`
on first-connect failure or on 15s of heartbeat-only traffic, and transparently reconnects with
`Last-Event-ID` afterward.

## Core types

### `AgentDef`

Named `AgentDef` rather than `Agent` to match this crate's existing `TaskDef`/`Task` and
`WorkflowDef`/`Workflow` convention: the `*Def` suffix means "a definition you serialize/register."

```rust
pub struct AgentDef {
    pub name: String,
    pub model: Option<String>,              // "provider/model"; None => external
    pub base_url: Option<String>,
    pub instructions: Option<Instructions>,  // enum: Text(String) | PromptTemplate(...)
    pub tools: Vec<ToolDef>,
    pub agents: Vec<AgentDef>,               // sub-agents
    pub strategy: Strategy,
    pub router: Option<Router>,              // Agent(Box<AgentDef>) | TaskName(String); required when strategy = Router
    pub output_type: Option<schemars::schema::RootSchema>,
    pub guardrails: Vec<Guardrail>,
    pub memory: Option<ConversationMemory>,
    pub max_turns: u32,                      // default 25, hard cap independent of `termination`
    pub max_tokens: Option<u32>,
    pub timeout_seconds: u64,                // default 0
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub termination: Option<TerminationCondition>,
    pub swarm_transitions: Vec<SwarmTransition>,   // Strategy::Swarm only
    pub allowed_transitions: HashMap<String, Vec<String>>,   // orthogonal safety net on top of swarm_transitions
    pub callbacks: Vec<Arc<dyn CallbackHandler>>,
    pub credentials: Vec<String>,            // declared names only, never resolved client-side
    pub required_tools: Vec<String>,
    pub context_window_budget: Option<u32>,
    pub masked_fields: Vec<String>,
    pub metadata: HashMap<String, Value>,
    // Strategy::PlanExecute only:
    pub planner: Option<Box<AgentDef>>,
    pub fallback: Option<Box<AgentDef>>,
    pub fallback_max_turns: Option<u32>,
    pub planner_context: Vec<PlanContext>,
    pub synthesize: bool,                    // default true
}

impl AgentDef {
    pub fn new(name: impl Into<String>) -> Result<Self>;   // validates name against ^[a-zA-Z_][a-zA-Z0-9_-]*$
    pub fn with_model(self, model: impl Into<String>) -> Self;
    pub fn with_tool(self, tool: ToolDef) -> Self;
    pub fn with_sub_agent(self, agent: AgentDef) -> Self;
    pub fn with_guardrail(self, g: Guardrail) -> Self;
    pub fn with_termination(self, t: TerminationCondition) -> Self;
    pub fn with_tool_credentials(self, tool_name: &str, names: Vec<String>) -> Self;
    pub fn with_credentials(self, names: Vec<String>) -> Self;
    // ...
}
```

Fallible construction (`Result<Self, ConductorError>`) surfaces validation at build time rather
than at serialize time: name regex, `router` required when `strategy = Router`, `planner`
required when `strategy = PlanExecute`, sub-agent name uniqueness, `max_turns >= 1`.

`RunSettings` is a small, separate per-run override — it never mutates the base `AgentDef`:

```rust
#[derive(Default)]
pub struct RunSettings {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub thinking_budget_tokens: Option<u32>,
}
```

### Strategy

```rust
pub enum Strategy {
    Handoff,       // default: the LLM freely picks the next agent
    Sequential,
    Parallel,
    Router,
    RoundRobin,
    Random,
    Swarm,
    Manual,
    PlanExecute,
}
```

There are three distinct ways one agent can use another — kept explicit in the API, not just in
docs, so they don't get conflated: `ToolDef::agent(child)` (inline call, returns a result, no
control transfer), `SwarmTransition` (rule-triggered control transfer, `Strategy::Swarm` only),
`Strategy::Handoff` (LLM-chosen control transfer, the default multi-agent mode).

The swarm-only rule-based transition type is named `SwarmTransition`, not `HandoffCondition` —
deliberate, so it doesn't share vocabulary with the unrelated `Strategy::Handoff`. Don't rename it
to look more "consistent"; the two are unrelated mechanisms wearing similar names on purpose.

```rust
pub enum SwarmTransition {
    OnToolResult { tool_name: String, target: String, result_contains: Option<String> },
    OnTextMention { text: String, target: String },
    OnCondition { target: String, predicate: Arc<dyn Fn(&SwarmContext) -> bool + Send + Sync> },
}
```

### Tools

No decorator; `#[tool]` is a proc macro (`conductor-macros`) modeled on the existing `#[worker]`
macro:

```rust
#[derive(JsonSchema, Deserialize)]
struct GetWeatherArgs {
    city: String,
    #[serde(default)]
    units: Option<String>,
}

#[tool(description = "Get current weather for a city")]
async fn get_weather(args: GetWeatherArgs) -> Result<Value> {
    // ...
}
// macro generates `fn get_weather_tool() -> ToolDef`
```

The parameter is one struct implementing `JsonSchema + DeserializeOwned` — not N flat scalar
parameters extracted from a function signature — giving full nested-struct/enum schema support
for free via `schemars` (already wired into `src/schema/mod.rs::generate_schema::<T>(strict)`).
This also matches how OpenAI/Anthropic tool-calling actually shapes a call: one JSON object in,
not N top-level keys. Description comes from the attribute string; there's no
"humanize-the-function-name" fallback.

Server-side tool constructors need no local worker at all — just config:

```rust
impl ToolDef {
    pub fn http(name: &str, url: &str) -> Self;
    pub fn mcp(name: &str, server_url: &str) -> Self;
    pub fn agent(agent: AgentDef) -> Self;   // inline sub-agent-as-tool call
    pub fn human(name: &str) -> Self;
    // rag: index_text / search_index, matching WorkflowTask::llm_index_text/llm_search_index
}
```

MCP tool calls stay server-resolved (`ListMcpTools`/`CallMcpTool` system tasks) — no local MCP
client or local MCP auth story for v1.

### Guardrails, termination, callbacks, memory

```rust
pub enum OnFail { Retry, Raise, Fix, Human }
pub enum Position { Input, Output }

pub struct GuardrailResult { pub passed: bool, pub message: String, pub fixed_output: Option<String> }

pub trait GuardrailCheck: Send + Sync {
    fn check(&self, content: &str) -> GuardrailResult;
}

pub struct RegexGuardrail { /* patterns, mode, ... */ }
pub struct LlmGuardrail { /* model, policy, ... */ }

pub struct Guardrail {
    position: Position,
    on_fail: OnFail,
    name: Option<String>,
    max_retries: u32,           // default 3, validated >= 1
    check: Box<dyn GuardrailCheck>,
}
impl From<RegexGuardrail> for Guardrail { /* OnFail::Fix intentionally not offered */ }
impl From<LlmGuardrail> for Guardrail { /* ... */ }
```

`RegexGuardrail` doesn't expose `OnFail::Fix` as a constructible option — a regex check has no
way to fix content, so leaving it out is a compile error instead of a silent no-op. Retry-budget
escalation (`OnFail::Retry` becomes `Raise` once `max_retries` is exhausted) lives directly on the
runtime type that drives the `DoWhile`-loop-equivalent compilation, next to the loop it governs.

`TerminationCondition` gets real `BitAnd`/`BitOr` operator overloads for `&`/`|` composition:

```rust
pub enum TerminationCondition {
    TextMention { text: String, case_sensitive: bool },
    StopMessage { stop_message: String },
    MaxMessage { max_messages: u32 },
    TokenUsage { max_total: Option<u32>, max_prompt: Option<u32>, max_completion: Option<u32> },
    And(Vec<TerminationCondition>),
    Or(Vec<TerminationCondition>),
}
impl std::ops::BitAnd for TerminationCondition { /* flattens nested And */ }
impl std::ops::BitOr for TerminationCondition { /* flattens nested Or */ }
```

`CallbackHandler` — six hook points, all defaulted to no-op (`None` = continue, `Some` =
short-circuit):

```rust
pub trait CallbackHandler: Send + Sync {
    fn on_agent_start(&self, ctx: &CallbackContext) -> Option<Value> { None }
    fn on_agent_end(&self, ctx: &CallbackContext) -> Option<Value> { None }
    fn on_model_start(&self, ctx: &CallbackContext) -> Option<Value> { None }
    fn on_model_end(&self, ctx: &CallbackContext) -> Option<Value> { None }
    fn on_tool_start(&self, ctx: &CallbackContext) -> Option<Value> { None }
    fn on_tool_end(&self, ctx: &CallbackContext) -> Option<Value> { None }
}
```

Callbacks are caller-side only — registered on `AgentDef.callbacks: Vec<Arc<dyn CallbackHandler>>`
but not serialized into `agentConfig`. They're explicitly non-durable: don't use as the only
record of an audit trail, since a process restart can interrupt local observers.

`ConversationMemory` — workflow-variable-backed, so it survives restarts:

```rust
pub struct ConversationMemory {
    pub messages: Vec<Message>,
    pub max_messages: Option<u32>,   // trims oldest non-system message first
}
```

### `AgentEvent`

A real tagged enum — every event type declares only the fields it actually has, so "which fields
can I read for a `ToolCall` event" is a compiler-checked match arm, not an `Option::unwrap()`
gamble. The 12 variants below were verified directly against the server's SSE event DTO and every
real emit call site, replacing an earlier, smaller sketch that had drifted from what the server
actually sends:

```rust
pub enum AgentEvent {
    Thinking { execution_id: String, content: String },
    ToolCall { execution_id: String, tool_name: String, args: Value },
    ToolResult { execution_id: String, tool_name: String, result: Value },
    Handoff { execution_id: String, target: String },
    Waiting { execution_id: String, pending_tool: Value },   // HITL pause; freeform payload, mixes snake_case/camelCase keys
    GuardrailPass { execution_id: String, guardrail_name: String },
    GuardrailFail { execution_id: String, guardrail_name: String, content: String },
    Error { execution_id: String, content: String, tool_name: String },
    Done { execution_id: String, output: Value },
    ContextCondensed { execution_id: String, content: String, messages_before: i32, messages_after: i32, exchanges_condensed: i32 },
    SubagentStart { execution_id: String, target: String, content: String },
    SubagentStop { execution_id: String, target: String, result: String },
}
```

`execution_id` is mandatory on every variant, not optional: `Handoff`/`Sequential`/`Parallel`
strategies put a pending `HUMAN` step in a nested sub-execution, and `approve`/`reject`/`respond`
must target that inner id, not the top-level handle's — making the field non-optional on every
variant means there's no code path that compiles without one in hand.

## Wire format (`AgentDef` → `agentConfig`)

Fields serialize with `#[serde(rename_all = "camelCase")]` (`max_turns` → `maxTurns`,
`context_window_budget` → `contextWindowBudget`, etc.). The invariant that matters most: **absent
optional/collection fields are omitted from the JSON entirely, never emitted as `null` or `[]`** —
enforced per-field with `#[serde(skip_serializing_if = "Option::is_none")]` /
`"Vec::is_empty"` / `"HashMap::is_empty"`. A few fields have narrower emission rules on top of
that: `strategy` is only emitted when the agent actually has sub-agents (`agents` non-empty or
`planner`/`fallback` set); `synthesize` is only emitted when `false` (the `true` default is never
written); `enable_planning` is only emitted when `true`.

`AgentDef` derives `Deserialize` as well as `Serialize` — there's no reason not to, since serde
derives both directions for free, and round-trip support is directly useful for reading back a
compiled/deployed agent later.

A contract test builds a "kitchen sink" `AgentDef`, serializes it, validates against the agent
JSON Schema, and asserts the exact set of emitted keys for known-empty vs. known-populated fields
— this is the test that actually enforces wire compatibility; treat it as required, not optional
polish.

## Credentials

This is the one part of the design that isn't a mechanical 1:1 translation of anything — it's
built around what's actually load-bearing: **the credential-delivery contract with the Conductor
server**, with no env var fallback, no `.env` file, no OS keyring, and no cloud secret-manager
client in the SDK itself.

1. **Declare** — a tool/agent lists credential *names* it needs: `ToolDef.credentials: Vec<String>`
   / `AgentDef.credentials: Vec<String>`. Never values.
2. **Register** — at task-definition registration time, the SDK stamps those names onto
   `TaskDef.runtime_metadata` (wire key `runtimeMetadata`), so the server knows this task type
   needs credentials resolved before dispatch.
3. **Resolve** — the server resolves each declared name against its own secret store.
4. **Deliver** — the server attaches resolved values to `Task.runtime_metadata` on the specific
   `Task` handed to a poll response — never persisted to task input, never a separate fetch call,
   never cached by the SDK.
5. **Consume** — the worker reads `task.runtime_metadata.get(name)`. Missing a declared name is a
   fail-closed error (`ConductorError::CredentialNotFound`), never a silent fallback to the
   process environment.

Model changes needed on the base task types, not agent-specific:

```rust
// src/models/task.rs
pub struct Task {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub runtime_metadata: HashMap<String, String>,   // server-populated, name -> resolved value
}

// src/models/task_def.rs
pub struct TaskDef {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_metadata: Vec<String>,               // SDK-populated, declared credential names
}
```

This assumes the target Conductor server supports `Task.runtimeMetadata` delivery — confirm the
minimum server version this SDK targets actually has this before relying on it in production;
otherwise gate the feature behind a version probe that fails fast with a clear error rather than
silently degrading to an insecure fallback.

### Primary path — resolved secrets as an explicit parameter

```rust
#[derive(JsonSchema, Deserialize)]
struct SearchArgs { query: String }

#[tool(description = "Search GitHub issues", credentials = ["GITHUB_TOKEN"])]
async fn search_github(args: SearchArgs, creds: &Credentials) -> Result<Value> {
    let token = creds.get("GITHUB_TOKEN")?;   // Result<&str, ConductorError::CredentialNotFound>
    let client = octocrab::Octocrab::builder().personal_token(token).build()?;
    // ...
}
```

`Credentials` is a plain, cheaply-constructed, read-only wrapper (`Arc<HashMap<String, String>>`
under the hood, built fresh from `task.runtime_metadata` once per poll) — no lock, because nothing
shared is mutated; concurrent tool executions each get their own `Credentials`, so there's no
serialization point to bottleneck on. Two touch points instead of one on purpose: the
`credentials = [...]` attribute is the declaration (what gets stamped onto
`TaskDef.runtime_metadata`), `creds.get(...)` is the read — the contract stays visible in the
tool's signature without reading the body, and there's no silent-miss case for a name built from
a non-literal expression.

When the credential name isn't a literal:

```rust
agent.with_tool_credentials("create_issue", vec!["GH_TOKEN".into()]);  // this tool
let agent = AgentDef::new("filer")?.with_credentials(vec!["GH_TOKEN".into()]);  // everything under this agent
```

### Secondary path — subprocess tools

If a tool shells out to an external process that only reads credentials from its own environment:

```rust
#[tool(description = "File a GitHub issue via gh CLI", credentials = ["GH_TOKEN"])]
async fn gh_create_issue(args: CreateIssueArgs, creds: &Credentials) -> Result<Value> {
    let output = tokio::process::Command::new("gh")
        .args(["issue", "create", "--title", &args.title])
        .env("GH_TOKEN", creds.get("GH_TOKEN")?)   // scoped to this child process only
        .output()
        .await?;
    Ok(json!({ "stdout": String::from_utf8_lossy(&output.stdout) }))
}
```

`Command::env()` is already scoped to the one child process being spawned — it never touches this
process's own environment, so there's nothing shared to lock and nothing a concurrent tool call
could clobber. No global `std::env::set_var`-based injection is used anywhere in this design:
that call is process-global and `unsafe` in a multithreaded process, and importing it here would
recreate a real hazard for no benefit, since nothing in this crate's own tool handlers has an
"only reads ambient env vars" problem to work around in the first place.

### Tertiary path — ambient accessor, offered but not the default

For deeply nested call stacks where threading a parameter through every layer is genuinely
annoying, a `tokio_task_local!`-scoped accessor is available as an opt-in convenience:

```rust
tokio::task_local! {
    static CURRENT_CREDENTIALS: Credentials;
}

pub fn current_credential(name: &str) -> Result<String> {
    CURRENT_CREDENTIALS.try_with(|c| c.get(name).map(str::to_string))
        .map_err(|_| ConductorError::internal("no credential context — are you inside a tool call?"))?
}
```

Task-local values do **not** automatically propagate across a `tokio::spawn` boundary — a handler
that spawns its own sub-tasks must re-scope the value explicitly
(`CURRENT_CREDENTIALS.scope(creds.clone(), async { ... })`) or pass `Credentials` down as a normal
parameter instead. The explicit-parameter path is the documented default in every example; the
task-local accessor stays available but isn't the first thing users see.

### What stays server-resolved and opaque to the SDK

`http_tool`/`api_tool`/`mcp_tool` credentials (referenced via `${NAME}` placeholders in headers)
are resolved entirely server-side — there's no local worker executing those calls at all
(`HttpTask`, `ListMcpTools`/`CallMcpTool` are server-side system tasks), so there's nothing for the
SDK to inject. The SDK's only job for this path is a client-side lint at `ToolDef` construction
time: validating that every `${NAME}` placeholder used in a header/config value is also present in
that tool's declared `credentials: Vec<String>`.

### Logging & redaction discipline

- `Credentials`'s `Debug`/`Display` impls show only the *names* it holds, never values.
- `ConductorError::CredentialNotFound(names)` carries names only.
- Nothing exposes a method that would let a resolved credential value end up in a
  `Task`/`TaskResult`, workflow variable, log line, or trace span — enforced by construction, not
  left to each tool author's discipline.

### Explicitly out of scope for v1

- Local OS keychain / credential cache — would introduce a second secret-storage story alongside
  the server's; revisit only for a concrete offline/local use case.
- Cloud secret-manager clients (AWS/GCP/Azure) in the SDK — server-managed secrets already cover
  this; a Conductor server can itself be configured against those providers.
- JWT minting/verification for tool-call requests — add only if a concrete signed-request
  requirement shows up.

## Worked examples

All three assume `--features agents` and a running Conductor server
(`Configuration::from_env()`).

### 1. Single agent, a tool, a guardrail

```rust
use conductor::agents::{AgentDef, AgentRuntime, Guardrail, OnFail, Position, RegexGuardrail};
use conductor::agents::tool;
use conductor::{Configuration, error::Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(JsonSchema, Deserialize)]
struct GetWeatherArgs {
    city: String,
}

#[tool(description = "Get current weather for a city")]
async fn get_weather(args: GetWeatherArgs) -> Result<Value> {
    Ok(json!({ "city": args.city, "condition": "sunny", "temp_f": 72 }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let mut runtime = AgentRuntime::new(config)?;

    let agent = AgentDef::new("weather_assistant")?
        .with_model("openai/gpt-4o")
        .with_instructions("You are a helpful weather assistant. Use the get_weather tool.")
        .with_tool(get_weather_tool())
        .with_guardrail(Guardrail::from(
            RegexGuardrail::new(&["(?i)i don't know"])
                .mode_block()
                .position(Position::Output)
                .on_fail(OnFail::Retry)
                .max_retries(2),
        ))
        .with_max_turns(10);

    let result = runtime
        .run(&agent, json!({ "prompt": "What's the weather in Austin?" }))
        .await?;

    println!("{}", result.output);
    runtime.shutdown().await
}
```

`get_weather_tool()` is generated by the `#[tool]` macro; its parameter schema comes from
`GetWeatherArgs`'s `JsonSchema` derive, not runtime introspection. `runtime.run(...)` compiles
`agent` to `agentConfig`, registers `get_weather_tool()` as a local worker on the runtime's
internal `TaskHandler`, starts the durable workflow, and blocks until terminal.

### 2. Multi-agent handoff with a credentialed tool

```rust
use conductor::agents::{AgentDef, AgentRuntime, Credentials};
use conductor::agents::tool;
use conductor::{Configuration, error::Result};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(JsonSchema, Deserialize)]
struct SearchArgs { query: String }

#[tool(description = "Search internal GitHub issues", credentials = ["GITHUB_TOKEN"])]
async fn search_github(args: SearchArgs, creds: &Credentials) -> Result<Value> {
    let token = creds.get("GITHUB_TOKEN")?;
    let client = octocrab::Octocrab::builder().personal_token(token).build()?;
    let page = client.search().issues_and_pull_requests(&args.query).send().await?;
    Ok(json!({ "results": page.items.len() }))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let mut runtime = AgentRuntime::new(config)?;

    let triage = AgentDef::new("triage")?
        .with_model("anthropic/claude-opus-4")
        .with_instructions("Classify the user's request, then hand off to the right specialist.");

    let github_specialist = AgentDef::new("github_specialist")?
        .with_model("anthropic/claude-opus-4")
        .with_instructions("You investigate GitHub issues using the search_github tool.")
        .with_tool(search_github_tool());

    let mut allowed = HashMap::new();
    allowed.insert("triage".to_string(), vec!["github_specialist".to_string()]);

    let coordinator = AgentDef::new("support_bot")?
        .with_model("anthropic/claude-opus-4")
        .with_sub_agent(triage)
        .with_sub_agent(github_specialist)
        .with_strategy(conductor::agents::Strategy::Handoff)
        .with_allowed_transitions(allowed)
        .with_max_turns(15);

    let handle = runtime
        .start(&coordinator, json!({ "prompt": "Find open issues mentioning 'rate limit'" }))
        .await?;

    let result = handle.join().await?;
    println!("{:?}", result.status);
    runtime.shutdown().await
}
```

`search_github`'s only path to the token is the `creds` parameter, resolved fresh per invocation
from that specific `Task`'s `runtime_metadata`. If `GITHUB_TOKEN` isn't registered as a secret on
the server, this fails closed with `ConductorError::CredentialNotFound(["GITHUB_TOKEN"])` before
`search_github` ever runs.

### 3. Streaming with approval

```rust
#[tool(description = "Issue a refund", approval_required = true)]
async fn issue_refund(args: RefundArgs) -> Result<Value> {
    billing::refund(&args.order_id, args.amount).await
}

let agent = AgentDef::new("support")?
    .with_model("anthropic/claude-sonnet-4-5")
    .with_instructions("Help with orders.")
    .with_tool(issue_refund_tool());

let input = json!({ "prompt": "Refund order A-1029, it arrived broken" });

// non-blocking, poll/stream it yourself — approval decision lives at the call site
let handle = runtime.start(&agent, input).await?;
let mut stream = handle.stream().await?;
while let Some(event) = stream.next().await.transpose()? {
    match event {
        AgentEvent::Waiting { execution_id, pending_tool }
            if pending_tool["tool_name"] == "issue_refund" =>
        {
            if pending_tool["parameters"]["amount"].as_f64().unwrap_or(0.0) < 100.0 {
                handle.approve(&execution_id).await?;
            } else {
                handle.reject(&execution_id, "Needs a manager").await?;
            }
        }
        AgentEvent::Done { output, .. } => println!("{output}"),
        _ => {}
    }
}
```

No `on_approval` block is registered on the agent up front — the decision is made at the call site
against the `Waiting` event, using that event's own `execution_id` (not the top-level handle's),
per the note on `AgentEvent` above. "Non-blocking with a callback when done" needs no bespoke API:
it's `tokio::spawn` wrapping `handle.join()`.
