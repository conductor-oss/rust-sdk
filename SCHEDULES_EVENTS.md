# Schedules and Events

Use `SchedulerClient` for cron-based workflow schedules and `EventClient` for event-driven
integration (event handlers, message queue configurations).

```rust
use conductor::models::SaveScheduleRequest;

let scheduler = client.scheduler_client();

let schedule = SaveScheduleRequest::new("nightly_report", "0 0 0 * * ?", "generate_report_workflow");
scheduler.save_schedule(&schedule).await?;
```

Give scheduled executions a stable correlation or idempotency key via the scheduled workflow's
own input so a retried or overlapping run doesn't duplicate business effects.

| Operation | Client / method |
|---|---|
| Create/update a schedule | `SchedulerClient::save_schedule` |
| Pause/resume one or all schedules | `pause_schedule` / `resume_schedule` / `pause_all_schedules` / `resume_all_schedules` |
| Delete a schedule | `SchedulerClient::delete_schedule` |
| List/search schedule executions | `get_all_schedules` / `search_schedule_executions` |
| Register an event handler | `EventClient::register_event_handler` |
| Manage a queue configuration (Kafka/SQS/AMQP) | `EventClient::put_queue_configuration` / `get_queue_configuration` |

See [WORKFLOW_LIFECYCLE.md](WORKFLOW_LIFECYCLE.md) for safe operational handling of the
workflows a schedule starts, and [WORKFLOW_MESSAGE_QUEUE.md](WORKFLOW_MESSAGE_QUEUE.md) for
pushing ad hoc messages into an already-running workflow instead of on a schedule.

## Related Documentation

- **[src/client/scheduler_client.rs](src/client/scheduler_client.rs)**
- **[src/client/event_client.rs](src/client/event_client.rs)**
- **[examples/schedule_journey.rs](examples/schedule_journey.rs)**
