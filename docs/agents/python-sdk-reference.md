# One-Pager: Agents in python-sdk (source of truth)

Package: `conductor.ai.agents` (+ `conductor.client.agent_client` / `conductor.client.orkes.orkes_agent_client`
for transport). This is the reference the Rust port must match — "Done when" in the checklist is
literally "same agent in this SDK and in Python produces identical `agentConfig`."

## Layering

```
AgentClient / OrkesAgentClient   transport only — POST/GET /agent/*, no knowledge of Agent objects
        ▲
AgentRuntime                     DX layer — compiles Agent → agentConfig, registers local tool
        │                        workers via the *same* TaskHandler every Conductor worker uses,
        │                        drives lifecycle (plan/deploy/serve/run/start/resume)
        ▼
Agent (+ RunSettings)            declarative definition — name, model, tools, guardrails,
                                 sub-agents, strategy, termination, ...
        │
AgentConfigSerializer            Agent → wire JSON (`agentConfig`). One-directional: no
                                 deserializer exists anywhere in the codebase.
```

`AgentRuntime` is not a passive worker — it's a control-plane client (`/agent/compile`,
`/agent/start`, `/agent/{id}/status`, SSE stream) **plus** a local task-worker host, because tool
execution stays local even though the LLM loop itself is compiled into a durable server-side
workflow (`DoWhile` + `LlmChatComplete` + one task per tool/guardrail/callback/router/etc).

## `AgentClient` (transport) — `client/agent_client.py`, `client/orkes/orkes_agent_client.py`

Every method has a sync + `_async` twin, all reusing the SDK's existing `X-Authorization` JWT
mint/refresh/401-retry — no bespoke auth.

| Method | HTTP | Purpose |
|---|---|---|
| `compile_agent(payload)` | `POST /agent/compile` | Agent config → agent/workflow def, no registration |
| `deploy_agent(payload)` | `POST /agent/deploy` | compile + register, no execution |
| `start_agent(payload)` | `POST /agent/start` | compile + register + execute |
| `get_status(id)` | `GET /agent/{id}/status` | point-in-time snapshot |
| `get_execution(id)` | `GET /agent/execution/{id}` | full execution tree |
| `list_executions(params)` | `GET /agent/executions` | search |
| `respond(id, body)` | `POST /agent/{id}/respond` | complete a pending HITL task |
| `stop(id)` | `POST /agent/{id}/stop` | graceful deterministic stop |
| `signal(id, message)` | `POST /agent/{id}/signal` | inject persistent context |
| `stream_sse(id, last_event_id?)` | `GET /agent/stream/{id}` | SSE event stream, falls back to `SSEUnavailableError` |

Errors: `ConductorAgentError` → `AgentAPIError` (any non-404) → `AgentNotFoundError` (404).
`SSEUnavailableError` is a separate root, raised on first-connect failure or 15s of
heartbeat-only traffic.

Registration: `OrkesClients.get_agent_client()` does a **lazy, function-local import** of
`OrkesAgentClient` specifically to keep the agent module off the hot import path for SDK users
who don't touch agents — every other `get_X_client()` imports eagerly at module top level.

## `Agent` — the definition object (`agent.py`)

Roughly 45 constructor fields. The ones that matter most for the Rust port's field list:

| Field | Type | Notes |
|---|---|---|
| `name` | `str` | = Conductor workflow name; regex `^[a-zA-Z_][a-zA-Z0-9_-]*$` |
| `model` | `str \| ClaudeCode` | `"provider/model"`; empty → `external=True` (server emits a `SubWorkflowTask` reference instead of a definition) |
| `instructions` | `str \| Callable \| PromptTemplate` | callable is invoked with no args at serialize time |
| `tools` | `List[ToolDef\|Callable]` | |
| `agents` | `List[Agent]` | sub-agents, model inherited from parent when unset |
| `strategy` | `Strategy` | `handoff\|sequential\|parallel\|router\|round_robin\|random\|swarm\|manual\|plan_execute` |
| `guardrails` | `List[Guardrail]` | |
| `termination` | `Optional[TerminationCondition]` | composable via `&`/`\|` |
| `handoffs` | `List[HandoffCondition]` | **swarm-strategy only** — see naming trap below |
| `allowed_transitions` | `Dict[str, List[str]]` | orthogonal safety net on top of handoffs |
| `callbacks` | `List[CallbackHandler]` | 6 hook points, chained, first-non-empty-dict wins |
| `memory` | `Optional[ConversationMemory]` | workflow-variable-backed, survives restarts |
| `credentials` | `List[str]` | **declared names only** — never resolved client-side at definition time |
| `max_turns` | `int = 25` | hard cap, independent of `termination` |
| `output_type` | `Optional[type]` | Pydantic/dataclass, validated server-side |
| `planner` / `fallback` / `planner_context` | | `strategy=plan_execute` only |

Two design traps worth carrying into the Rust naming:
- **`Strategy.HANDOFF`** (the default multi-agent strategy — LLM freely picks the next agent)
  and **`HandoffCondition`** (rule-based transitions, only meaningful under `strategy=swarm`)
  share vocabulary but are unrelated mechanisms.
- Everything cross-references by **string name** (`HandoffCondition.target`,
  `allowed_transitions` keys/values, `agent_tool`'s sub-workflow registration) — nothing is
  validated at construction time against the actual `agents=[...]` list.

Three distinct ways one agent can "use" another, easily conflated: `agent_tool(child)` (inline
call, returns a result, no control transfer), `HandoffCondition` (rule-triggered control
transfer, swarm only), `Strategy.HANDOFF` (LLM-chosen control transfer, the default).

## Composition primitives

| Concept | File | Shape |
|---|---|---|
| Tool | `tool.py` | `@tool` decorator (schema from function signature) *or* server-side factories (`http_tool`, `mcp_tool`, `api_tool`, `human_tool`, `agent_tool`, RAG tools) that need no local worker at all |
| Schema derivation | `_internal/schema_utils.py` | Type hints → JSON Schema; **no support for Enum/dataclass/Pydantic-model function parameters** — degrades to `{}` (unconstrained) |
| Guardrail | `guardrail.py` | `Position{input,output}` × `OnFail{retry,raise,fix,human}`; built-ins `RegexGuardrail`, `LLMGuardrail`; retry-budget escalation (`on_fail=retry` → `raise` once `max_retries` exhausted) lives in the dispatcher, not the class |
| Termination | `termination.py` | `TextMention`, `StopMessage`, `MaxMessage`, `TokenUsage`; composable, compiles to a `DoWhile` loop condition |
| Handoff | `handoff.py` | `OnToolResult`, `OnTextMention`, `OnCondition` — swarm-only, see naming trap above |
| Callback | `callback.py` | `on_agent_start/end`, `on_model_start/end`, `on_tool_start/end`; explicitly non-durable ("do not use as the only record of an audit... process restarts can interrupt local observers") |
| Memory | `memory.py` | `ConversationMemory` — the only one actually wired in, workflow-variable-backed. `semantic_memory.py`'s `SemanticMemory`/`MemoryStore` (pluggable vector backend) is defined but **has no `Agent` constructor parameter and no runtime wiring** — dead/aspirational code, don't treat its docstring example as real behavior. |

## Wire format (`config_serializer.py` → `agentConfig`)

One-directional (`Agent → dict`, no deserializer exists). Hand-written per-field camelCase
mapping (not reflective). The load-bearing contract, confirmed by
`tests/unit/ai/test_config_serializer.py`: **absent fields are omitted from the dict entirely,
never emitted as `null` or `[]`.** `agent-schema.json` (root `additionalProperties: false`,
only `name` required) is the secondary, test-checked artifact.

Two confirmed schema/serializer mismatches, untested by either test file — resolve which side is
authoritative before the Rust port encodes either shape as truth:
- `allowedTransitions`: serializer emits a dict (`{agent_name: [names]}`); schema declares
  `array<string>`.
- `planSource`: serializer emits a dict; schema declares `string`.

## Runtime & credentials — see [`secrets-and-credentials.md`](secrets-and-credentials.md)

The short version: workers are Python subprocesses (`multiprocessing.spawn`), which is why large
parts of the runtime (`_worker_entries.py`'s pickling/spawn-safety machinery,
`inject_via_env()`'s `os.environ` mutation + process-wide lock) exist. None of that is
architecture worth porting — it's a workaround for a constraint Rust's single-process async
worker model doesn't have. The one thing that *is* load-bearing and must port: **credentials are
declared by name on `Tool`/`Agent`, resolved server-side, and delivered to the worker only via
`Task.runtime_metadata` at poll time — never via env-var fallback, never cached.**

## Frameworks — see [`framework-support.md`](framework-support.md)

Two structurally different integration modes, chosen per-object by a priority-ordered heuristic
with silent fallback: **full extraction** (walk the foreign object, produce a real `agentConfig`,
Conductor's own guardrail/termination loop drives execution) vs. **passthrough** (the foreign
framework's own `.invoke()`/`.stream()` loop runs inside one opaque Conductor task; Conductor only
observes via callback/hook events). LangGraph's extraction path depends on CPython-specific
bytecode/closure introspection with no Rust analog. Claude Agent SDK is always passthrough (it
shells out to the `claude` CLI over stdio).
