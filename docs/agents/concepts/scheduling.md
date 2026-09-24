# Scheduling

**Audience:** operators scheduling recurring agent executions.

## Prerequisites

Deploy the agent first, choose a stable, unique-per-agent schedule name, and use idempotent
workflow input for every scheduled run.

Build one or more [`Schedule`]s and reconcile them against a deployed agent in one call:

```rust
use conductor::agents::Schedule;

let daily = Schedule::new("daily_report", "0 0 9 * * *")?
    .with_timezone("America/Los_Angeles");

runtime.deploy_with_schedules(&agent, Some(&[daily])).await?;
```

`deploy_with_schedules` follows a tri-state contract for its `schedules` argument:

| `schedules` | Effect |
|---|---|
| `None` | Leave this agent's existing schedules untouched (same as plain `deploy`). |
| `Some(&[])` | Delete every schedule currently registered for this agent. |
| `Some(non-empty)` | Upsert the listed schedules; delete any existing schedule for this agent not in the list. |

The wire-level schedule name the server stores is `"{agent_name}-{short_name}"` (see
[`wire_name`]) — this is a plain `-`-join shared with every Conductor SDK, so a schedule name
containing a `-` at the wrong spot can theoretically collide with a different
agent/short-name pair. Not fixable from one SDK alone; use schedule names without embedded
`-` where that risk matters.

## Inspecting schedules

`list_schedules(scheduler_client, agent_name)` returns each schedule as a [`ScheduleInfo`],
which reports the short name (with the agent prefix stripped) alongside pause state, cron
expression, and timestamps.

## Expected result and cleanup

A saved schedule starts the deployed agent on its cron cadence. Pause or delete a schedule
(via `Some(&[])` or an updated list) before removing the agent definition or retiring the
worker service that serves its tools.

## Next steps

Read [runtime modes](deploy-serve-run.md).
