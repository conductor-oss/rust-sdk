# Schedule Management API Reference

Complete API reference for schedule management operations in the Conductor Rust SDK.

## Table of Contents

- [Quick Start](#quick-start)
- [SchedulerClient API](#schedulerclient-api)
- [Managing Schedules](#managing-schedules)
- [Schedule Control](#schedule-control)
- [Schedule Execution](#schedule-execution)
- [Tagging](#tagging)
- [Models Reference](#models-reference)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::{SaveScheduleRequest, StartWorkflowRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("http://localhost:8080/api");
    let client = ConductorClient::new(config)?;
    let scheduler_client = client.scheduler_client();

    // Create a schedule to run daily at midnight
    let workflow_request = StartWorkflowRequest::new("daily_report")
        .with_input_value("date", "${scheduler.scheduledTime}");

    let schedule = SaveScheduleRequest::new("daily_report_schedule", "0 0 0 * * ?")
        .with_description("Generate daily reports at midnight")
        .with_start_workflow_request(workflow_request)
        .with_timezone("America/New_York");

    scheduler_client.save_schedule(&schedule).await?;
    println!("Schedule created successfully!");

    Ok(())
}
```

---

## SchedulerClient API

### Schedule Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `save_schedule()` | `POST /scheduler/schedules` | Create or update a schedule |
| `get_schedule()` | `GET /scheduler/schedules/{name}` | Get schedule by name |
| `get_all_schedules()` | `GET /scheduler/schedules` | Get all schedules |
| `delete_schedule()` | `DELETE /scheduler/schedules/{name}` | Delete a schedule |

### Schedule Control

| Method | Endpoint | Description |
|--------|----------|-------------|
| `pause_schedule()` | `GET /scheduler/schedules/{name}/pause` | Pause a schedule |
| `pause_all_schedules()` | `GET /scheduler/admin/pause` | Pause all schedules |
| `resume_schedule()` | `GET /scheduler/schedules/{name}/resume` | Resume a schedule |
| `resume_all_schedules()` | `GET /scheduler/admin/resume` | Resume all schedules |

### Execution Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `get_next_few_schedule_execution_times()` | `GET /scheduler/nextFewSchedules` | Get next execution times |
| `search_schedule_executions()` | `GET /scheduler/search/executions` | Search execution history |
| `requeue_all_execution_records()` | `GET /scheduler/admin/requeue` | Requeue all executions |

### Tag Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `set_scheduler_tags()` | `PUT /scheduler/schedules/{name}/tags` | Set schedule tags |
| `get_scheduler_tags()` | `GET /scheduler/schedules/{name}/tags` | Get schedule tags |
| `delete_scheduler_tags()` | `DELETE /scheduler/schedules/{name}/tags` | Delete schedule tags |

---

## Managing Schedules

### Create a Schedule

```rust
use conductor::models::{SaveScheduleRequest, StartWorkflowRequest};

// Create workflow request
let workflow_request = StartWorkflowRequest::new("order_processing")
    .with_version(1)
    .with_input_value("batch_size", 100)
    .with_input_value("scheduled_time", "${scheduler.scheduledTime}")
    .with_correlation_id("daily-batch-${scheduler.scheduledTime}");

// Create schedule using Spring cron format (6 fields)
let schedule = SaveScheduleRequest::new(
    "daily_order_processing",
    "0 0 0 * * ?",  // Daily at midnight (second minute hour day month weekday)
)
.with_description("Process pending orders daily")
.with_start_workflow_request(workflow_request)
.with_timezone("UTC");

scheduler_client.save_schedule(&schedule).await?;
```

### Update a Schedule

```rust
// Get existing schedule
let existing = scheduler_client.get_schedule("daily_order_processing").await?;

// Create updated schedule
let workflow_request = StartWorkflowRequest::new("order_processing")
    .with_input_value("batch_size", 200);  // Increased batch size

let schedule = SaveScheduleRequest::new(
    "daily_order_processing",
    "0 0 */2 * * ?",  // Every 2 hours now
)
.with_description("Process orders every 2 hours")
.with_start_workflow_request(workflow_request);

scheduler_client.save_schedule(&schedule).await?;
```

### Get a Schedule

```rust
let schedule = scheduler_client.get_schedule("daily_order_processing").await?;

println!("Schedule: {}", schedule.name);
println!("Cron: {}", schedule.cron_expression);
println!("Paused: {}", schedule.paused);
if let Some(next) = schedule.schedule_start_time {
    println!("Next run: {}", next);
}
```

### Get All Schedules

```rust
// Get all schedules
let schedules = scheduler_client.get_all_schedules(None).await?;

for schedule in &schedules {
    println!("{}: {} - {}", 
        schedule.name, 
        schedule.cron_expression,
        if schedule.paused { "PAUSED" } else { "ACTIVE" });
}

// Get schedules for a specific workflow
let workflow_schedules = scheduler_client.get_all_schedules(Some("order_processing")).await?;
```

### Delete a Schedule

```rust
scheduler_client.delete_schedule("daily_order_processing").await?;
println!("Schedule deleted");
```

---

## Schedule Control

### Pause Schedule

```rust
// Pause a specific schedule
scheduler_client.pause_schedule("daily_order_processing").await?;
println!("Schedule paused");
```

### Resume Schedule

```rust
// Resume a specific schedule
scheduler_client.resume_schedule("daily_order_processing").await?;
println!("Schedule resumed");
```

### Pause All Schedules

```rust
// Emergency: pause all schedules
scheduler_client.pause_all_schedules().await?;
println!("All schedules paused");
```

### Resume All Schedules

```rust
// Resume all schedules
scheduler_client.resume_all_schedules().await?;
println!("All schedules resumed");
```

---

## Schedule Execution

### Get Next Execution Times

Test a cron expression to see when it will trigger:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

let next_times = scheduler_client.get_next_few_schedule_execution_times(
    "0 0 0 * * ?",    // Cron expression
    Some(now),         // Start time
    None,              // End time (optional)
    Some(5),           // Number of times to return
).await?;

for timestamp in next_times {
    let dt = chrono::DateTime::from_timestamp_millis(timestamp).unwrap();
    println!("Next execution: {}", dt);
}
```

### Search Execution History

```rust
let results = scheduler_client.search_schedule_executions(
    Some(0),                          // start offset
    Some(20),                         // page size
    Some("startTime:DESC"),           // sort order
    None,                             // free text search
    Some("scheduleName='daily_order_processing'"),  // query filter
).await?;

println!("Total executions: {}", results.total_hits);
for execution in results.results {
    println!("  {} - {}: {:?}", 
        execution.schedule_name,
        execution.workflow_id.unwrap_or_default(),
        execution.state);
}
```

### Requeue Execution Records

```rust
// Requeue all pending execution records
scheduler_client.requeue_all_execution_records().await?;
```

---

## Tagging

### Set Schedule Tags

```rust
use conductor::models::MetadataTag;

let tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("team", "operations"),
    MetadataTag::new("priority", "high"),
];

scheduler_client.set_scheduler_tags(&tags, "daily_order_processing").await?;
```

### Get Schedule Tags

```rust
let tags = scheduler_client.get_scheduler_tags("daily_order_processing").await?;

for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}
```

### Delete Schedule Tags

```rust
let tags_to_delete = vec![
    MetadataTag::new("priority", "high"),
];

scheduler_client.delete_scheduler_tags(&tags_to_delete, "daily_order_processing").await?;
```

---

## Models Reference

### SaveScheduleRequest

```rust
use conductor::models::SaveScheduleRequest;

let schedule = SaveScheduleRequest::new("schedule_name", "0 0 * * * ?")
    .with_description("Description")
    .with_start_workflow_request(workflow_request)
    .with_timezone("America/New_York")
    .with_paused(false)
    .with_schedule_start_time(start_epoch_ms)
    .with_schedule_end_time(end_epoch_ms);
```

**Fields:**
- `name`: Unique schedule identifier
- `cron_expression`: Spring cron expression (6 fields)
- `description`: Optional description
- `start_workflow_request`: Workflow to execute
- `zone_id`: Timezone (default: UTC)
- `paused`: Start paused (default: false)
- `schedule_start_time`: When schedule becomes active (epoch ms)
- `schedule_end_time`: When schedule expires (epoch ms)

### WorkflowSchedule

```rust
pub struct WorkflowSchedule {
    pub name: String,
    pub cron_expression: String,
    pub zone_id: String,
    pub paused: bool,
    pub start_workflow_request: Option<StartWorkflowRequest>,
    pub schedule_start_time: Option<i64>,
    pub schedule_end_time: Option<i64>,
    pub create_time: Option<i64>,
    pub updated_time: Option<i64>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
}
```

### Spring Cron Format

Conductor uses Spring cron format with 6 fields:

```
┌─────────── second (0 - 59)
│ ┌───────── minute (0 - 59)
│ │ ┌─────── hour (0 - 23)
│ │ │ ┌───── day of month (1 - 31)
│ │ │ │ ┌─── month (1 - 12)
│ │ │ │ │ ┌─ day of week (0 - 7, SUN-SAT)
│ │ │ │ │ │
* * * * * *
```

**Examples:**
- `0 0 0 * * ?` - Daily at midnight
- `0 0 * * * ?` - Every hour at minute 0
- `0 0 9 * * MON-FRI` - Weekdays at 9 AM
- `0 */15 * * * ?` - Every 15 minutes
- `0 0 0 1 * ?` - First day of every month
- `0 0 0,12 * * ?` - Midnight and noon

---

## Best Practices

### 1. Use Descriptive Names

```rust
// Good
let schedule = SaveScheduleRequest::new(
    "daily_order_processing_midnight_utc",
    "0 0 0 * * ?",
);

// Bad
let schedule = SaveScheduleRequest::new(
    "sched1",
    "0 0 0 * * ?",
);
```

### 2. Test Cron Expressions

```rust
async fn validate_cron(client: &SchedulerClient, cron: &str) -> Result<bool> {
    let times = client.get_next_few_schedule_execution_times(
        cron,
        None,
        None,
        Some(5),
    ).await?;
    
    if times.is_empty() {
        println!("Warning: Cron expression '{}' has no upcoming executions", cron);
        return Ok(false);
    }
    
    for t in &times {
        println!("  {}", chrono::DateTime::from_timestamp_millis(*t).unwrap());
    }
    
    Ok(true)
}
```

### 3. Handle Timezones Correctly

```rust
// Always specify timezone explicitly
let schedule = SaveScheduleRequest::new("my_schedule", "0 0 9 * * ?")
    .with_timezone("America/New_York")  // Business hours in NYC
    .with_description("Daily at 9 AM Eastern");

// For UTC-based schedules, be explicit
let schedule = SaveScheduleRequest::new("global_schedule", "0 0 0 * * ?")
    .with_timezone("UTC")
    .with_description("Daily at midnight UTC");
```

### 4. Use Tags for Organization

```rust
// Tag by environment and team
let prod_tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("team", "operations"),
    MetadataTag::new("criticality", "high"),
    MetadataTag::new("oncall", "ops-team@example.com"),
];

scheduler_client.set_scheduler_tags(&prod_tags, "critical_schedule").await?;
```

### 5. Implement Idempotent Workflows

```rust
// Use scheduledTime in correlation ID for idempotency
let workflow_request = StartWorkflowRequest::new("daily_process")
    .with_correlation_id("daily-process-${scheduler.scheduledTime}")
    .with_input_value("scheduled_time", "${scheduler.scheduledTime}");
```

### 6. Monitor Schedule Health

```rust
async fn check_schedule_health(client: &SchedulerClient) -> Result<()> {
    let schedules = client.get_all_schedules(None).await?;
    
    let active = schedules.iter().filter(|s| !s.paused).count();
    let paused = schedules.iter().filter(|s| s.paused).count();
    
    println!("Schedules: {} active, {} paused", active, paused);
    
    // Check for recently failed executions
    let results = client.search_schedule_executions(
        Some(0),
        Some(100),
        Some("startTime:DESC"),
        None,
        Some("state=FAILED"),
    ).await?;
    
    if results.total_hits > 0 {
        println!("Warning: {} failed executions", results.total_hits);
    }
    
    Ok(())
}
```

---

## See Also

- [Workflow Management](./WORKFLOW.md) - Workflows being scheduled
- [Metadata Management](./METADATA.md) - Workflow definitions
- [Authorization](./AUTHORIZATION.md) - Schedule permissions
