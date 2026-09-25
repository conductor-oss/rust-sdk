# Multi-agent systems

**Audience:** authors composing specialists into a durable Conductor-agent graph.

## Prerequisites

Every sub-agent needs a unique name among its siblings and a bounded execution policy (`max_turns`
plus, for open-ended designs, a [`TerminationCondition`](termination.md)). Start with a single
agent and a tested tool before introducing delegation.

## Strategies

Set `strategy` via `AgentDef::with_strategy`, after adding sub-agents/tools/router/planner as
each strategy requires:

| Strategy | Behavior |
|---|---|
| `Handoff` (default) | The LLM freely picks the next agent. |
| `Sequential` / `Parallel` | Deterministic composition over `agents`; the parent needs a model for aggregation — inherited automatically from the first child that has one if the parent doesn't set its own. |
| `Router` | A router sub-agent (set via `with_router`, required before `with_strategy(Strategy::Router)`) picks the next agent. |
| `RoundRobin` / `Random` | Fixed rotation / random selection among sub-agents. |
| `Swarm` | Rule-based transitions via [`SwarmTransition`] (see below). |
| `Manual` | The caller selects the next agent externally. |
| `PlanExecute` | A planner sub-agent (set via `with_planner`, required first) produces a JSON plan the parent executes — see [Plans](#plans) below. |

Three distinct ways one agent uses another: `ToolDef::agent(child)` (inline call, returns a
result, no control transfer), [`SwarmTransition`] (rule-triggered transfer, `Strategy::Swarm`
only), `Strategy::Handoff` (LLM-chosen transfer, the default). `allowed_transitions` restricts
which swarm targets each agent may reach, independent of `swarm_transitions` itself.

```rust
use conductor::agents::{AgentDef, Strategy, SwarmTransition};

let team = AgentDef::new("bug_desk")?
    .with_strategy(Strategy::Swarm)?
    .with_swarm_transition(SwarmTransition::OnTextMention {
        target: "filer".into(),
        text: "ACTIONABLE".into(),
    });
```

`SwarmTransition::OnToolResult` fires after a named tool call, optionally narrowed to results
containing a substring; `OnCondition` runs an arbitrary predicate over a [`SwarmContext`].

## Conditional pipelines

`AgentDef::with_gate` (a declarative `TextGate`, compiled server-side) or `with_gate_fn` (an
arbitrary async predicate, registered as a worker by `AgentRuntime::serve`) stops a sequential
pipeline after an agent if its output does or doesn't satisfy a condition — useful for
early-exit branches inside `Strategy::Sequential`.

## Plans

For deterministic, inspectable execution, build a [`Plan`] directly instead of letting an LLM
plan freely:

```rust
use conductor::agents::{plan_execute, Op, Plan, PlanExecuteOptions, Step};

let harness = plan_execute(
    "research_team",
    vec![search_tool, summarize_tool],
    PlanExecuteOptions {
        planner_instructions: "Search, then summarize the results.".into(),
        ..Default::default()
    },
)?;
```

`plan_execute` builds the planner/fallback/parent trio in one call. Pass a pre-built `Plan` as
`static_plan` on `AgentRuntime::start`'s `input` to skip the planner LLM entirely and run a
fully deterministic pipeline — steps declare `depends_on` and `Ref` wires one step's output
into another's input.

## Expected result and failures

The compiled parent contains durable child/sub-workflow work; each child is visible in the
execution history. A `Sequential`/`Parallel` parent with no model anywhere in itself or its
children fails at `with_strategy` time, before ever reaching the server. If a graph loops, add
`max_turns`, [termination](termination.md), and a bounded plan/fallback policy before retrying.

## Next steps

Read [termination](termination.md), [stateful agents](stateful.md), and the
[agent-definition reference](../reference/agent-definition.md).
