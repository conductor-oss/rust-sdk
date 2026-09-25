# Agent definition fields

`AgentDef` accepts a name, `provider/model`, instructions, tools, sub-agents, and runtime
policy. Construct with `AgentDef::new` and compose with consuming `with_*` builders.

Names must match `^[a-zA-Z_][a-zA-Z0-9_-]*$`, since the name doubles as the compiled workflow
name. An unset `model` is only valid for a `Sequential`/`Parallel` parent that inherits one
from a sub-agent, or an agent marked via `with_framework`.

| Field | Notes |
|---|---|
| `tools`, `agents` | See [tools](../concepts/tools.md) and [multi-agent](../concepts/multi-agent.md). |
| `strategy`, `router`, `swarm_transitions`, `allowed_transitions` | See [multi-agent](../concepts/multi-agent.md). Router requires `with_router` first; `PlanExecute` requires `with_planner` and at least one tool first. |
| `guardrails` | See [guardrails](../concepts/guardrails.md). |
| `termination` | See [termination](../concepts/termination.md). |
| `memory` | See [stateful agents](../concepts/stateful.md). |
| `callbacks` | See [callbacks](../concepts/callbacks.md). Registered but never serialized onto the wire. |
| `credentials` | Declared names only, applying to every tool under this agent — see [tools](../concepts/tools.md#credentials). |
| `max_turns` (default 25), `max_tokens`, `timeout_seconds`, `temperature`, `reasoning_effort` | Run-tuning fields; overridable per run via `RunSettings`. |
| `output_type` | Structured-output schema, set via `with_output_type(class_name, schema)`. |
| `planner`, `fallback`, `fallback_max_turns`, `planner_context`, `synthesize` | `Strategy::PlanExecute`-only — see [multi-agent](../concepts/multi-agent.md#plans). |
| `gate`, `stop_when` | Conditional-pipeline and early-stop predicates — see [multi-agent](../concepts/multi-agent.md#conditional-pipelines). |
| `introduction`, `include_contents` | Group-conversation self-introduction text, and whether a sub-agent inherits the parent's context. |
| `prefill_tools` | Tool calls executed before the first LLM turn, with results injected into context. |

`AgentDef` implements `Serialize`/`Deserialize`; absent optional/collection fields are omitted
from the wire representation entirely, never emitted as `null` or `[]`. The complete
construction and validation semantics are maintained in `src/agents/def.rs` and
`AgentConfigSerializer`; use those sources when adding a newly supported field.
