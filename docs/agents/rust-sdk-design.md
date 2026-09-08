# One-Pager: Agents in rust-sdk (target design)

Status: proposal. Nothing here is implemented. Cross-checked against current rust-sdk
conventions (`Deref`-based Orkes extension, `.with_x()` builders, `thiserror` + `ConductorError`,
`async-trait` `Worker`, `schemars`-based schema generation, flat `mod`+`pub use` client
registration, hierarchical env-var config) so this slots in rather than introducing a second
style.

This is purely additive — nothing in the existing crate (the `LlmChatComplete`/`LlmTextComplete`
workflow-task builders, `agentic_workflow.rs`/`multiagent_chat.rs` examples) implements an
"Agent" abstraction today; those stay as the low-level workflow DSL that the new layer sits above.

## Goals

- Same `agentConfig` JSON on the wire as python-sdk, for the same agent (checklist "Done when").
- Feel like the rest of this SDK: builders, `Result<T, ConductorError>`, `async-trait` workers,
  `schemars` for schema derivation — not a transliteration of Python's dynamic-typing tricks.
- Fix the places where faithfully copying Python would mean copying a workaround for a
  constraint Rust doesn't have (multiprocessing/pickling) or a bug (see § Wire compatibility).

## Non-goals for v1

- Bit-for-bit parity with every python-sdk framework passthrough (LangGraph bytecode
  introspection, Claude Agent SDK CLI passthrough) — see [`framework-support.md`](framework-support.md).
  Native agent authoring (the `AgentDef` path) is full parity target; framework adapters are
  phased.
- Local MCP client. MCP tool calls stay server-resolved (`ListMcpTools`/`CallMcpTool` system
  tasks), same as python-sdk today — no local Rust MCP auth story needed yet.
- `SemanticMemory`. Python's own version isn't wired into `Agent` either (see reference doc) —
  don't invent a connected feature Python doesn't actually have.

## Feature flag

Gate the whole module behind a Cargo feature, `agents`, following the existing `macros`
precedent. Reasoning: python-sdk deliberately lazy-imports `OrkesAgentClient` so non-agent users
don't pay import weight; Rust's equivalent is compile-time feature-gating, which is strictly
better (zero cost when unused, not just deferred cost).

```toml
[features]
default = []
agents = []
```

## Module layout

```
src/
├── agents/
│   ├── mod.rs                 # pub use surface
│   ├── def.rs                 # AgentDef, RunSettings, Strategy
│   ├── tool.rs                # ToolDef, Tool trait, tool! / #[tool] (see below)
│   ├── guardrail.rs            # Guardrail, RegexGuardrail, LlmGuardrail, GuardrailResult
│   ├── termination.rs          # TerminationCondition + BitAnd/BitOr composition
│   ├── handoff.rs               # SwarmTransition (Strategy::Swarm only — no type named for Strategy::Handoff, which is model-driven and needs no condition object)
│   ├── callback.rs             # CallbackHandler trait, CallbackPosition
│   ├── memory.rs               # ConversationMemory
│   ├── serializer.rs           # AgentDef -> agentConfig (Serialize impls + contract test)
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── runtime.rs          # AgentRuntime: plan/deploy/serve/run/start/resume
│   │   ├── config.rs           # AgentRuntimeConfig (env-hierarchy, mirrors WorkerConfig)
│   │   └── credentials.rs      # see secrets-and-credentials.md
│   └── result.rs               # AgentResult, AgentStatus, AgentHandle, AgentEvent, AgentStream
├── client/
│   └── agent_client.rs         # AgentClient (thin transport, matches SecretClient/PromptClient shape)
└── models/
    ├── task.rs                 # ADD: Task.runtime_metadata: HashMap<String,String>  (base-SDK change, see secrets doc)
    └── task_def.rs             # ADD: TaskDef.runtime_metadata: Vec<String>
```

`agents/` is a new top-level module, not nested under `worker/` or `client/`, because it spans
both (it needs a transport client *and* a worker host) — same reasoning python-sdk applied by
keeping `conductor.ai.agents` separate from `conductor.client`.

## Core types

### `AgentDef` (not `Agent`)

Named `AgentDef` rather than `Agent`, deliberately diverging from python's naming, because this
crate already has a `TaskDef`/`Task` and `WorkflowDef`/`Workflow` split: the `*Def` suffix means
"a definition you serialize/register," the bare name means "a runtime instance." Python's
`Agent` is structurally a definition (it *is* what serializes to `agentConfig`), so `AgentDef` is
the name consistent with this crate's own convention, not an arbitrary rename.

Builder pattern, matching `TaskDef::new(...).with_x(...)`:

```rust
pub struct AgentDef {
    pub name: String,
    pub model: Option<String>,              // "provider/model"; None => external
    pub base_url: Option<String>,
    pub instructions: Option<Instructions>,  // enum: Text(String) | PromptTemplate(...)
    pub tools: Vec<ToolDef>,
    pub agents: Vec<AgentDef>,
    pub strategy: Strategy,
    pub router: Option<Router>,              // Agent(Box<AgentDef>) | TaskName(String)
    pub output_type: Option<schemars::schema::RootSchema>,
    pub guardrails: Vec<Guardrail>,
    pub memory: Option<ConversationMemory>,
    pub max_turns: u32,                      // default 25
    pub max_tokens: Option<u32>,
    pub timeout_seconds: u64,                // default 0
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub termination: Option<TerminationCondition>,
    pub swarm_transitions: Vec<SwarmTransition>,  // Strategy::Swarm only
    pub allowed_transitions: HashMap<String, Vec<String>>,
    pub callbacks: Vec<Arc<dyn CallbackHandler>>,
    pub credentials: Vec<String>,            // declared names only
    pub required_tools: Vec<String>,
    pub context_window_budget: Option<u32>,
    pub masked_fields: Vec<String>,
    pub metadata: HashMap<String, Value>,
    // plan_execute-only:
    pub planner: Option<Box<AgentDef>>,
    pub fallback: Option<Box<AgentDef>>,
    pub fallback_max_turns: Option<u32>,
    pub planner_context: Vec<PlanContext>,
    pub synthesize: bool,                    // default true
    // ...
}

impl AgentDef {
    pub fn new(name: impl Into<String>) -> Result<Self>;  // validates name regex up front
    pub fn with_model(self, model: impl Into<String>) -> Self;
    pub fn with_tool(self, tool: ToolDef) -> Self;
    pub fn with_sub_agent(self, agent: AgentDef) -> Self;
    pub fn with_guardrail(self, g: Guardrail) -> Self;
    pub fn with_termination(self, t: TerminationCondition) -> Self;
    // ...
}
```

`RunSettings` ports as-is — it's already a small, clean override struct:

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

Validation Python does at `__init__` time moves to `AgentDef::new`/builder methods returning
`Result<Self, ConductorError>` where it can fail (name regex, `router` required when
`strategy=Router`, `planner` required when `strategy=PlanExecute`, sub-agent name uniqueness,
`max_turns >= 1`) — same checks, enforced at construction instead of scattered through a 150-line
`__init__`.

### Strategy — and the `Handoff` naming trap

```rust
pub enum Strategy {
    Handoff,       // default: LLM freely picks the next agent
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

Keep `Strategy::Handoff` (it's the right name for what it does — matches the wire string
`"handoff"`), but rename the swarm-only condition type from python's `HandoffCondition` to
**`SwarmTransition`**. Python's own code has two unrelated things sharing the word "handoff"; the
research explicitly flagged this as worth disambiguating in the Rust API rather than
reproducing. `agents.rs` module doc should say this out loud so nobody "fixes" it back to match
Python by name.

```rust
pub enum SwarmTransition {
    OnToolResult { tool_name: String, target: String, result_contains: Option<String> },
    OnTextMention { text: String, target: String },
    OnCondition { target: String, predicate: Arc<dyn Fn(&SwarmContext) -> bool + Send + Sync> },
}
```

Three ways one agent uses another — keep the distinction explicit in the API, not just in docs:
`ToolDef::agent(child)` (inline call, returns a result), `SwarmTransition` (rule-triggered
control transfer, `Strategy::Swarm` only), `Strategy::Handoff` (LLM-chosen control transfer, the
default multi-agent mode).

### Tools — schema derivation is a genuine improvement over Python here

No decorator equivalent (`@tool`). Two paths, matching python's two paths but with Rust's own
idioms:

**A. Function-backed tool**, modeled on the existing `#[worker]` proc macro (`conductor-macros`) —
same `darling`/`syn`/`quote` skeleton, retargeted:

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

The parameter is **one struct implementing `JsonSchema + DeserializeOwned`**, not N flat
scalar parameters extracted from a function signature. This is a deliberate divergence from
python's `schema_from_function` (which derives per-parameter schemas from type hints and has no
support at all for nested Enum/dataclass/Pydantic-model parameters — see reference doc). Rust
gets full nested-struct/enum schema support for free via `schemars`, already a crate dependency,
already wired into `src/schema/mod.rs::generate_schema::<T>(strict)`. It also matches how
OpenAI/Anthropic tool-calling actually shapes a call (one JSON object in, not N top-level keys) —
this is strictly better parity with the LLM-facing contract, not just a Rust convenience.

**B. Server-side tool constructors** — no local worker, just config, matching python's
`http_tool`/`mcp_tool`/`api_tool`/`human_tool`/`agent_tool`:

```rust
impl ToolDef {
    pub fn http(name: &str, url: &str) -> Self;
    pub fn mcp(name: &str, server_url: &str) -> Self;
    pub fn agent(agent: AgentDef) -> Self;   // inline sub-agent-as-tool call
    pub fn human(name: &str) -> Self;
    // rag: index_text / search_index, matching existing WorkflowTask::llm_index_text/llm_search_index
}
```

`ToolDef.credentials: Vec<String>` carries declared credential names through to
`TaskDef.runtime_metadata` at registration time — see secrets doc.

### Guardrails, termination, callbacks, memory

Same shapes as python, translated to enums/traits instead of duck-typed classes:

```rust
pub enum OnFail { Retry, Raise, Fix, Human }
pub enum Position { Input, Output }

pub struct GuardrailResult { pub passed: bool, pub message: String, pub fixed_output: Option<String> }

pub trait GuardrailCheck: Send + Sync {
    fn check(&self, content: &str) -> GuardrailResult;
}

pub struct RegexGuardrail { patterns: Vec<Regex>, mode: RegexMode, ... }
pub struct LlmGuardrail { model: String, policy: String, ... }

// The common wrapper every concrete check attaches to — holds the fields python puts on its
// `Guardrail` base class (position, on_fail, name, max_retries) plus the check itself, so
// `AgentDef.guardrails: Vec<Guardrail>` is homogeneous regardless of which check backs it.
pub struct Guardrail {
    position: Position,
    on_fail: OnFail,
    name: Option<String>,
    max_retries: u32,           // default 3, validated >= 1
    check: Box<dyn GuardrailCheck>,
}
impl From<RegexGuardrail> for Guardrail { /* on_fail: Fix intentionally not offered, see below */ }
impl From<LlmGuardrail> for Guardrail { /* ... */ }
```

One deliberate fix, not a faithful bug-for-bug port: python's `RegexGuardrail` silently accepts
`on_fail: "fix"` but never implements a fix (behaves like `raise`). In Rust, don't expose
`OnFail::Fix` as a constructible option on `RegexGuardrail` at all — make it a compile error
instead of a silent no-op. Same for the retry-budget escalation logic (`on_fail: Retry` becomes
`Raise` once `max_retries` is exhausted) — python buries this in the dispatcher, separate from
the `Guardrail` class itself; in Rust, put it directly on whatever type actually runs the
DoWhile-loop-equivalent compilation, so the escalation rule lives next to the loop it governs
rather than two files away.

`TerminationCondition` gets real operator overloads via `std::ops::{BitAnd, BitOr}`, which is a
closer match to python's `&`/`|` ergonomics than most Rust translations get to be for free:

```rust
pub enum TerminationCondition {
    TextMention { text: String, case_sensitive: bool },
    StopMessage { stop_message: String },
    MaxMessage { max_messages: u32 },
    TokenUsage { max_total: Option<u32>, max_prompt: Option<u32>, max_completion: Option<u32> },
    And(Vec<TerminationCondition>),
    Or(Vec<TerminationCondition>),
}
impl std::ops::BitAnd for TerminationCondition { /* flattens nested And, like python */ }
impl std::ops::BitOr for TerminationCondition { /* flattens nested Or */ }
```

`CallbackHandler` is a trait with six methods, all defaulted to no-op (`Option<Value>` returns,
`None` = continue, `Some` = short-circuit — same override semantics as python, minus the
legacy single-callback kwargs, which don't need porting since they're already deprecated
upstream):

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

`ConversationMemory` ports directly — it's already a small, clean struct
(`messages: Vec<Message>`, `max_messages: Option<u32>`, trim-oldest-non-system-first) backed by
Conductor workflow variables, same storage model as python.

## Wire compatibility (the "Done when" bar)

Name Rust struct fields so `#[serde(rename_all = "camelCase")]` produces the exact wire keys
python emits (`max_turns`→`maxTurns`, `context_window_budget`→`contextWindowBudget`, etc. — every
field python hand-writes a camelCase mapping for happens to already be a mechanical
snake_case→camelCase transform, so the derive macro can do in one line what python's serializer
does field-by-field).

The invariant that actually matters, confirmed by python's own contract test
(`test_none_values_omitted`): **absent optional/collection fields must be omitted from the JSON
entirely, never emitted as `null` or `[]`.** Use `#[serde(skip_serializing_if = "Option::is_none")]`
/ `"Vec::is_empty"` / `"HashMap::is_empty"` per field to match. Two more python-specific
serialization quirks to replicate deliberately (they're behavior, not accidents):
`strategy` is only emitted when the agent actually has sub-agents (`agents` non-empty or
`planner`/`fallback` set); `synthesize` is only emitted when `false` (the `true` default is never
written); `enable_planning` is only emitted when `true`.

Two confirmed mismatches between python's serializer and its own `agent-schema.json` — **resolve
which side is authoritative with the python-sdk maintainers before encoding either as truth** in
the Rust struct/serde impl, rather than guessing:

- `allowedTransitions`: serializer emits `HashMap<String, Vec<String>>`; schema says
  `array<string>`.
- `planSource`: serializer emits an object; schema says `string`.

Add a contract test mirroring python's `test_agent_schema_contract.py` +
`test_config_serializer.py`: build a "kitchen sink" `AgentDef`, serialize it, validate against
`agent-schema.json` (a JSON Schema validator crate, e.g. `jsonschema` or `valico`, would be a new
dev-dependency), and assert the exact set of emitted keys for known-empty vs. known-populated
fields. This is the test that actually enforces the checklist's "Done when" line — don't treat it
as optional polish.

Unlike python, there's no reason **not** to add `Deserialize` too (serde derives both directions
for free) — do it. Python has zero deserialization code because nothing needed it; Rust gets
round-trip support, which is directly useful for the contract test above and for reading back a
compiled/deployed agent later. This is an enhancement, not something requiring a Python reference
to copy.

## `AgentClient` — matches the existing thin-client shape

Same shape as `SecretClient`/`PromptClient`: a `#[derive(Clone)] struct { api: ApiClient }`,
registered in `client/mod.rs` + a `ConductorClient::agent_client()` accessor. No `Deref`-based
Orkes extension needed — unlike `MetadataClient`/`OrkesMetadataClient`, python-sdk doesn't have a
plain-OSS vs. Orkes split for agents (the whole feature is Orkes-only), so one concrete
`AgentClient` is enough; no base/extension pair.

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

`stream_events` wraps SSE (`reqwest`'s streaming body) with the same fallback contract python
has: raise/return `ConductorError::SseUnavailable` on first-connect failure or on 15s of
heartbeat-only traffic, transparent reconnect with `Last-Event-ID` afterward. This is genuinely
new plumbing for `ApiClient` (today's `add_auth_header`/request methods aren't built for a
streaming response) — budget real design/implementation time for it, it's not a thin wrapper.

## `AgentRuntime` — composition over the existing worker framework, not a parallel one

The strongest reuse point the research turned up: **an agent-invocation poller is an ordinary
`impl Worker`.** Don't build a second polling loop. `AgentRuntime` owns one `TaskHandler`
internally (exactly the relationship python's `AgentRuntime`↔`WorkerManager`↔`TaskHandler` already
has), and every tool/guardrail/router/callback that needs a local worker gets registered onto
that same `TaskHandler` — which means agent workers get pooling (`Semaphore`+`thread_count`),
panic isolation, retry-on-update, graceful shutdown, and Prometheus metrics for free, identically
to every other worker in this SDK.

```rust
pub struct AgentRuntime {
    agent_client: AgentClient,
    task_handler: TaskHandler,
    config: AgentRuntimeConfig,
}

impl AgentRuntime {
    pub fn new(configuration: Configuration) -> Result<Self>;
    pub fn with_config(configuration: Configuration, config: AgentRuntimeConfig) -> Result<Self>;

    pub async fn plan(&self, agent: &AgentDef) -> Result<Value>;                    // compile only
    pub async fn deploy(&mut self, agent: &AgentDef) -> Result<DeploymentInfo>;      // + register + local workers, no exec
    pub async fn serve(&mut self, agents: &[AgentDef]) -> Result<()>;               // deploy + block
    pub async fn run(&mut self, agent: &AgentDef, input: Value) -> Result<AgentResult>;   // + execute, block to result
    pub async fn start(&mut self, agent: &AgentDef, input: Value) -> Result<AgentHandle>; // + execute, don't block
    pub async fn resume(&mut self, execution_id: &str) -> Result<AgentHandle>;       // reattach after restart
    pub async fn shutdown(self) -> Result<()>;
}
```

Same six verbs as python (`plan`/`deploy`/`serve`/`run`/`start`/`resume`), same reasoning: `deploy`
for CI/CD, `serve` for long-lived processes, `run` for quickstart. `AgentRuntimeConfig` mirrors
`WorkerConfig`'s env-hierarchy resolution (`CONDUCTOR_AGENT_{NAME}_{PROP}` →
`CONDUCTOR_AGENT_ALL_{PROP}` → code default) — same pattern, new prefix, no new mechanism.

`AgentHandle`/`AgentResult`/`AgentStatus`/`AgentEvent`/`AgentStream` port close to 1:1 from
python's `result.py` — that hierarchy is language-agnostic already (fields, not behavior tied to
Python). One shape change: python's `AgentEvent` is a flat dataclass with a `type: EventType`
discriminant field and a pile of `Optional[...]` fields that only some event types populate.
Rust should make this a real tagged `enum` instead — every event type declares only the fields it
actually has, so "which fields am I allowed to read for a `tool_call` event" is a compiler-checked
match arm, not an `Option::unwrap()` gamble:

```rust
pub enum AgentEvent {
    Thinking { execution_id: String, content: String },
    ToolCall { execution_id: String, tool_name: String, args: Value },
    ToolResult { execution_id: String, tool_name: String, result: Value },
    Handoff { execution_id: String, target: String },
    Waiting { execution_id: String, tool_name: String, args: Value },   // HITL pause
    Message { execution_id: String, content: String },
    GuardrailPass { execution_id: String, guardrail_name: String },
    GuardrailFail { execution_id: String, guardrail_name: String, message: String },
    Error { execution_id: String, message: String },
    Done { execution_id: String, output: Value },
}
```

Preserve the one load-bearing subtlety: HITL responses (`respond`/`approve`/`reject`) must carry
the **event's `execution_id`**, not the handle's top-level one, because
`Handoff`/`Sequential`/`Parallel` strategies put the pending `HUMAN` task in a nested
sub-execution. Making `execution_id` a field on every variant (rather than optional, rather than
only on some variants) means the `respond`/`approve`/`reject` call sites are forced to have one
in hand — there's no code path that compiles without it, which is stronger than a doc comment
someone can miss.

### What's new here vs. what maps onto existing infra

| Concern | Maps onto existing rust-sdk infra | Needs new code |
|---|---|---|
| Poll for agent-invocation tasks | `Worker` + `TaskHandler` — direct reuse | — |
| Multi-turn "come back later" | `WorkerOutput::InProgress` + `TaskContext::poll_count()` — direct reuse | — |
| Concurrency, panic isolation, metrics | `TaskRunner`'s semaphore/panic-catch/event-dispatch — direct reuse | — |
| Tool schema derivation | `schemars` + `src/schema/mod.rs::generate_schema` — direct reuse | — |
| Compile/deploy/status/HITL control plane | `ApiClient` request plumbing — direct reuse | new `AgentClient` (boilerplate, no new pattern) |
| In-process tool-call dispatch loop | — | new: nothing today models "run N sub-calls inside one `Worker::execute()`" |
| SSE event streaming | — | new: `ApiClient` has no streaming-response path today |
| Credential delivery to workers | — | new: `Task.runtime_metadata` doesn't exist on the model yet — see secrets doc |

## Error handling

Extend `ConductorError` following its existing `thiserror` + constructor-fn + `is_retryable()`
convention — don't introduce a parallel error type:

```rust
pub enum ConductorError {
    // ...existing variants...
    Agent(String),
    ToolExecution(String),
    CredentialNotFound(Vec<String>),   // matches python's fail-closed, name-only-in-error contract
    GuardrailFailed(String),
    SseUnavailable(String),
}
```

`CredentialNotFound` deliberately carries only *names*, never values, matching python's explicit
"don't leak values" logging discipline (see secrets doc).

## Required base-SDK model changes

The credential-delivery contract (declare names → server resolves → deliver on the polled `Task`)
is a base-SDK change, not an agents-specific one — needed regardless of how much of the Agent
surface ships in v1:

- `models/task.rs`: add `Task.runtime_metadata: HashMap<String, String>` (server-populated,
  wire key `runtimeMetadata`).
- `models/task_def.rs`: add `TaskDef.runtime_metadata: Vec<String>` (SDK-populated at
  registration time, from `ToolDef.credentials`).

Also two pre-existing, unrelated small gaps worth fixing alongside this work since they touch the
same enum/builder pattern: `TaskType::McpRemote` has no `WorkflowTask::mcp_remote(...)` builder,
and `TaskType::LlmGetEmbeddings`/`LlmStoreEmbeddings` have no builders either, despite the enum
variants already existing.

## Workflow DSL — checklist § F

The low-level builder side of this (`WorkflowTask::llm_chat_complete`/`llm_text_complete`/etc.,
mirroring python's `task_type.py` + `workflow/task/llm_tasks/`) already exists in this crate —
see § Reusable building blocks above. The one piece from checklist §F not yet covered anywhere:
a `start_agent` `EventHandler` action, referenced there only as a java-sdk change
(`EventHandler.StartAgent`, java-sdk#168) — an event handler that starts an agent execution in
response to a workflow/task event, independent of the `AgentClient.start_agent` HTTP call this
doc already covers. Java-sdk wasn't available locally during this research pass; **confirm the
exact `EventHandler` action JSON shape against java-sdk before implementing** the corresponding
`EventHandlerAction`/`StartAgent` variant in `src/models/`, rather than guessing the shape from
the name alone.

## Testing & docs — checklist section G

Mirror python-sdk's structure directly, same numbering where it maps:
- `docs/agents/` (this directory) → user-facing docs once implemented, one file per concept
  (matches this repo's existing flat-per-feature convention: `SECRET_MANAGEMENT.md`,
  `PROMPT.md`, etc.) rather than python's nested `concepts/`/`reference/`/`frameworks/` — adapt
  the *content* coverage, not the literal directory nesting, to fit how this repo already
  organizes docs.
- `examples/agent_*.rs` → same numbering/names as python's `examples/agents/` where a direct
  analog exists.
- `tests/agent_*.rs` (unit) + an integration test mirroring
  `tests/integration/test_agentic_workflows.py` (end-to-end against a live server, HITL +
  streaming) — required by the checklist's "Done when," not optional.
- CI: copy `.github/workflows/agent-e2e.yml`'s intent (an E2E job against a live Conductor
  server), adapted to this repo's existing CI conventions.
