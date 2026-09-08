# Secrets & Credentials — design doc

This is the one part of the port that is explicitly **not** a 1:1 translation. Python's mechanism
is a direct product of two things Rust doesn't have: a multiprocessing worker model (pickling
across `spawn` boundaries) and a language-wide convention of libraries reading `os.environ`
implicitly. Rust has neither constraint, so faithfully porting the mechanism would mean porting
workarounds for problems that don't exist here, while missing the one part that's genuinely
load-bearing: **the credential-delivery contract with the Conductor server.**

## The one thing that must port exactly: the delivery contract

Across every credential path in python-sdk (native `@tool` functions, LangChain/LangGraph/Claude
Agent SDK framework adapters), there is exactly **one** source of truth: the Conductor server.
Nothing else — no env var fallback, no `.env` file, no OS keyring, no cloud secret manager client
in the SDK itself.

1. **Declare** — a tool/agent lists credential *names* it needs: `ToolDef.credentials: Vec<String>`
   / `AgentDef.credentials: Vec<String>`. Never values.
2. **Register** — at task-definition registration time, the SDK stamps those names onto
   `TaskDef.runtime_metadata` (wire key `runtimeMetadata`) so the server knows this task type
   needs credentials resolved before dispatch.
3. **Resolve** — the server resolves each declared name against its own secret store
   (encrypted at rest and in transit — server-side responsibility, not the SDK's).
4. **Deliver** — the server attaches resolved values to `Task.runtime_metadata` on the specific
   `Task` instance handed to a poll response. Never persisted to task input, never a separate
   fetch call, never cached by the SDK.
5. **Consume** — the worker reads `task.runtime_metadata.get(name)`. Missing a declared name is a
   fail-closed error (`ConductorError::CredentialNotFound`), not a warning, and not a fallback to
   the process environment.

This is the contract described in python-sdk's `security.md`: *"Agent tools declare required
credentials; a capable server delivers resolved values only in task runtime metadata. Missing
credentials fail before tool execution."* Keep the wording — it's a good security invariant.

### Prerequisite: server capability

This entire design assumes the target Conductor server supports `Task.runtimeMetadata` delivery
(python-sdk's porting spec ties this to a specific server-side change, referenced as
conductor-oss server PR #1255 / "Agents server > 0.4.2"). **Confirm the minimum server version
this SDK targets actually has this before implementing** — if it predates the capability, either
gate the feature behind a version probe (fail fast with a clear error, don't silently degrade to
an insecure fallback) or block the whole credentials story on a server-side dependency.

## Base-SDK model change required

This is not an agents-only change — add it to the core task models regardless of how much of the
Agent surface ships first:

```rust
// src/models/task.rs
pub struct Task {
    // ...
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub runtime_metadata: HashMap<String, String>,   // server-populated, name -> resolved value
}

// src/models/task_def.rs
pub struct TaskDef {
    // ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_metadata: Vec<String>,               // SDK-populated, declared credential names
}
```

## What does *not* port: `os.environ` mutation

Python's `inject_via_env()` mutates `os.environ` for the duration of a framework call (because
Google ADK, LangChain, LangGraph, and the Claude Agent SDK CLI all read env vars internally), then
restores it, the whole thing serialized behind one process-wide `threading.RLock` — explicitly
because two workers mutating the same global map concurrently is a real bug they'd already hit
once. Python's own docs call this out as a scaling limit: "strictly serial within a worker
process — scale by adding worker processes."

None of that applies here, but not because Rust magically avoids the problem — because **the
Rust design should never create the problem in the first place**:

- `std::env::set_var` is process-global exactly like Python's `os.environ`, and is `unsafe` in a
  multithreaded process as of recent Rust editions. Reaching for it to replicate Python's Tier 2
  would import the exact hazard Python's own lock exists to paper over, with a worse safety story
  (a lock around an `unsafe` call is not a good foundation).
- Rust tool handlers are plain async functions/closures with real parameters. There is no
  pickling boundary, no "the framework SDK we're calling only reads ambient env vars" problem to
  work around for anything written *for* this SDK, because nothing in the Rust ecosystem
  equivalent to LangChain/LangGraph/ADK exists yet that this SDK would need to accommodate (see
  [`framework-support.md`](framework-support.md)).

So: **skip Python's Tier 2 entirely.** Go straight to what python-sdk itself defined as its
preferred-but-never-wired-up "Tier 1" (`ExplicitSecrets` / `factory_accepts_secrets`) and make it
the *only* tier.

## Proposed Rust design: explicit, typed, no ambient state

### Primary path — resolved secrets as an explicit parameter

A tool handler that needs a credential takes it as a normal parameter, resolved by the runtime
before the call:

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
shared is mutated. Concurrent tool executions each get their own `Credentials` built from their
own `Task`; there is no serialization point to bottleneck on, which is a strictly better
concurrency story than python's "one lock per process" model, not just a different one.

`Credentials::get(name)` returns `Result<&str, ConductorError>`, failing closed exactly like
python: any declared name not present in `task.runtime_metadata` is
`ConductorError::CredentialNotFound(vec![name])`, which fails the task — no fallback to
`std::env::var()`, ever. This should be enforced in code, not just documented: the `#[tool]` macro
can validate at registration time that every name a handler asks `Credentials::get` for (or, more
practically, every name declared via `credentials = [...]`) matches the tool's own declared list.

### Secondary path — subprocess tools (the one case that legitimately needs env vars)

If a tool shells out to an external process that only reads credentials from its environment
(the direct Rust analog of python's Claude Agent SDK / CLI passthrough case), Rust's process API
already gives you a properly scoped answer with no global mutation at all:

```rust
tokio::process::Command::new("some-cli")
    .env("API_KEY", creds.get("API_KEY")?)   // scoped to this child process only
    .spawn()?;
```

`Command::env()` sets the variable only in the *child's* environment; the parent process's
`std::env` is never touched, so there's no shared mutable state, no lock, and no risk of one
concurrent tool call clobbering another's credentials — a direct structural improvement over
python's global-env-plus-lock approach, achieved by using an API Rust already has rather than by
designing anything new.

### Tertiary path — ambient accessor, offered but not the default

For parity with python's `get_secret(name)` context-var accessor (useful for deeply nested call
stacks where threading a parameter through every layer is genuinely annoying), a `tokio_task_local!`
-scoped accessor can be offered as an opt-in convenience:

```rust
tokio::task_local! {
    static CURRENT_CREDENTIALS: Credentials;
}

pub fn current_credential(name: &str) -> Result<String> {
    CURRENT_CREDENTIALS.try_with(|c| c.get(name).map(str::to_string))
        .map_err(|_| ConductorError::internal("no credential context — are you inside a tool call?"))?
}
```

Document its one real caveat clearly: task-local values do **not** automatically propagate across
a `tokio::spawn` boundary the way python's `contextvars` propagate through `asyncio` tasks — a
tool handler that spawns its own sub-tasks must re-scope the value explicitly
(`CURRENT_CREDENTIALS.scope(creds.clone(), async { ... })`) or pass `Credentials` down as a normal
parameter instead. Given that caveat, recommend the explicit-parameter path as the documented
default in every example and in the `#[tool]` macro's generated signature; keep the task-local
accessor available but not the first thing users see.

## Logging & redaction discipline

Match python's discipline exactly — this part has nothing language-specific about it:

- `Credentials`'s `Debug`/`Display` impls show only the *names* it holds, never values.
- `ConductorError::CredentialNotFound(names)` — names only, matching python's
  `CredentialNotFoundError`.
- Never write a resolved credential value into a `Task`/`TaskResult`, workflow variable, log line,
  or trace span. This should be enforced by construction (don't expose a method that would let a
  value end up in any of those) rather than left to each tool author's discipline.

## What stays server-resolved and opaque to the SDK

`http_tool`/`api_tool`/`mcp_tool` credentials (referenced via `${NAME}` placeholders in headers)
are resolved **entirely server-side** today in python-sdk and should stay that way in the Rust
port for v1 — there is no local worker executing those calls at all (`HttpTask`,
`ListMcpTools`/`CallMcpTool` are server-side system tasks), so there's nothing for the SDK to
inject. The SDK's only job for this path is validating, at `ToolDef` construction time, that every
`${NAME}` placeholder used in a header/config value is also present in that tool's declared
`credentials: Vec<String>` — a client-side lint, not a resolution step.

## Explicitly out of scope for v1

- **Local OS keychain / credential cache.** No `keyring`-style crate exists in this workspace
  today, and adding one would introduce a second secret-storage story alongside the server's,
  which cuts against the single-source-of-truth design above. Revisit only if a concrete use case
  needs offline/local credential storage independent of a live Conductor server.
- **Cloud secret manager clients (AWS/GCP/Azure) in the SDK.** Python-sdk doesn't have these
  either — server-managed secrets already cover this; a Conductor server can itself be configured
  against those providers.
- **JWT minting/verification for tool-call requests.** No `jsonwebtoken`/`ring` dependency exists
  today; add only if a concrete signed-request requirement shows up.

## Summary table: python mechanism → Rust equivalent

| Python | Rust | Why the change |
|---|---|---|
| `os.environ` mutation + process-wide `RLock` (`inject_via_env`) | Explicit `Credentials` parameter, no lock | No ambient env-reading libraries to accommodate; avoids the exact hazard the lock exists to paper over |
| `contextvars.ContextVar` (`get_secret`) | Optional `tokio::task_local!` accessor, documented as secondary | Same ergonomic goal, different propagation semantics — must be called out, not silently assumed equivalent |
| `ExplicitSecrets`/`factory_accepts_secrets` (defined, never wired up) | The *only* and default path | Rust has no reason not to use the mechanism python designed but never finished wiring in |
| ad hoc per-subprocess env passing (none — Claude Agent SDK bridge sets real process env) | `Command::env()` scoped to the child process | Rust's process API already gives per-child scoping; no new design needed |
| Execution-token + `/workers/secrets` fetch (retired even in python) | N/A | Don't resurrect a retired design |
| `Task.runtime_metadata` delivery | `Task.runtime_metadata` delivery (unchanged) | This is the actual contract worth preserving — it's server-side and language-agnostic |
