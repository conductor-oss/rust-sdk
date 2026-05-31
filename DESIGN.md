# Conductor Rust SDK - Design & API Documentation

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Module Structure](#module-structure)
4. [Configuration](#configuration)
5. [Client APIs](#client-apis)
6. [Worker Framework](#worker-framework)
7. [Models](#models)
8. [LLM/AI Tasks](#llmai-tasks-orkes)
9. [HTTP Poll Task](#http-poll-task)
10. [Wait for Webhook Task](#wait-for-webhook-task)
11. [State Change Events](#state-change-events)
12. [Fork/Join with Script](#forkjoin-with-script)
13. [Events System](#events-system)
14. [Metrics](#metrics)
15. [Error Handling](#error-handling)
16. [Examples](#examples)

---

## Overview

The Conductor Rust SDK is a comprehensive, async-first client library for Netflix Conductor workflow orchestration platform. It follows the same architecture as the [Python SDK](https://github.com/conductor-oss/python-sdk) while leveraging Rust's strengths in type safety and async programming.

### Key Features

- **Async/Await Support**: Built on Tokio runtime for high-performance async I/O
- **Batch Polling**: Efficiently poll multiple tasks in a single request
- **Automatic Retry**: Configurable retry logic with exponential backoff for task updates
- **Event-Driven Architecture**: Publish/subscribe events for monitoring and metrics
- **Prometheus Metrics**: Built-in metrics collection with HTTP endpoint
- **Type-Safe Models**: Strongly typed workflow and task definitions
- **Hierarchical Configuration**: Environment variable support with worker-specific overrides

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Application Layer                            │
├─────────────────────────────────────────────────────────────────────┤
│  WorkerHost / TaskHandler                                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                  │
│  │ TaskRunner  │  │ TaskRunner  │  │ TaskRunner  │  ...             │
│  │ (Worker 1)  │  │ (Worker 2)  │  │ (Worker N)  │                  │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                  │
│         │                │                │                          │
│         └────────────────┼────────────────┘                          │
│                          │                                           │
│                          ▼                                           │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    EventDispatcher                             │  │
│  │  ┌─────────────────┐  ┌─────────────────┐                     │  │
│  │  │ MetricsCollector│  │ Custom Listener │  ...                │  │
│  │  └─────────────────┘  └─────────────────┘                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│                          Client Layer                                │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                     ConductorClient                            │  │
│  │  ┌─────────────┐  ┌────────────────┐  ┌──────────────────┐   │  │
│  │  │ TaskClient  │  │ WorkflowClient │  │  MetadataClient  │   │  │
│  │  └──────┬──────┘  └───────┬────────┘  └────────┬─────────┘   │  │
│  │         └─────────────────┼───────────────────┘              │  │
│  └───────────────────────────┼───────────────────────────────────┘  │
│                              ▼                                       │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                        ApiClient                               │  │
│  │  • HTTP request/response handling                              │  │
│  │  • Authentication token management                             │  │
│  │  • Automatic token refresh                                     │  │
│  │  • Error handling & retries                                    │  │
│  └───────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│                        Conductor Server                              │
└─────────────────────────────────────────────────────────────────────┘
```

### Design Principles

1. **Separation of Concerns**: Clear boundaries between HTTP, client, worker, and application layers
2. **Composition over Inheritance**: Use traits and composition for extensibility
3. **Fail-Fast with Recovery**: Immediate error reporting with configurable retry mechanisms
4. **Observable by Default**: Built-in events and metrics for monitoring

---

## Module Structure

```
rust-sdk/
├── src/
│   ├── lib.rs                 # Public API exports
│   ├── configuration/
│   │   ├── mod.rs
│   │   ├── settings.rs        # Configuration struct
│   │   └── worker_config.rs   # Worker-specific config
│   ├── client/
│   │   ├── mod.rs
│   │   ├── conductor_client.rs # Main client entry point
│   │   ├── task_client.rs      # Task operations
│   │   ├── workflow_client.rs  # Workflow operations
│   │   └── metadata_client.rs  # Definition management
│   ├── http/
│   │   ├── mod.rs
│   │   └── api_client.rs      # Low-level HTTP client
│   ├── worker/
│   │   ├── mod.rs
│   │   ├── worker_trait.rs    # Worker trait & FnWorker
│   │   ├── task_runner.rs     # Polling loop implementation
│   │   ├── task_handler.rs    # Multi-worker management
│   │   └── worker_host.rs     # High-level hosting API
│   ├── models/
│   │   ├── mod.rs
│   │   ├── task.rs            # Task, TaskStatus, TaskExecLog
│   │   ├── task_def.rs        # TaskDef, RetryLogic, TimeoutPolicy
│   │   ├── task_result.rs     # TaskResult, TaskResultStatus
│   │   ├── workflow.rs        # Workflow, WorkflowStatus
│   │   └── workflow_def.rs    # WorkflowDef, WorkflowTask, TaskType
│   ├── events/
│   │   ├── mod.rs
│   │   ├── dispatcher.rs      # EventDispatcher
│   │   └── task_runner_events.rs # Event types & listener trait
│   ├── metrics/
│   │   ├── mod.rs
│   │   ├── settings.rs        # MetricsSettings
│   │   └── collector.rs       # MetricsCollector (Prometheus)
│   └── error.rs               # ConductorError, Result<T>
├── examples/
│   ├── hello_world.rs         # Basic usage
│   ├── async_workers.rs       # Multiple workers with metrics
│   └── dynamic_workflow.rs    # Dynamic workflow execution
└── tests/
    └── integration_tests.rs   # Integration tests
```

---

## Configuration

### Configuration Struct

```rust
use conductor::Configuration;
use std::time::Duration;

// From environment variables
let config = Configuration::from_env();

// Programmatic configuration
let config = Configuration::new("http://localhost:8080/api")
    .with_auth("key", "secret")
    .with_timeout(Duration::from_secs(30))
    .with_debug(true);
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CONDUCTOR_SERVER_URL` | Conductor server API URL | `http://localhost:8080/api` |
| `CONDUCTOR_AUTH_KEY` | Authentication key | None |
| `CONDUCTOR_AUTH_SECRET` | Authentication secret | None |
| `CONDUCTOR_DEBUG` | Enable debug logging | `false` |
| `CONDUCTOR_TIMEOUT_SECS` | Request timeout (seconds) | `30` |

### Worker Configuration

Worker-specific settings can be configured via environment variables with hierarchical precedence:

1. **Worker-specific**: `CONDUCTOR_WORKER_{WORKER_NAME}_{PROPERTY}`
2. **Global**: `CONDUCTOR_WORKER_ALL_{PROPERTY}`
3. **Code defaults**

| Variable Pattern | Description |
|-----------------|-------------|
| `CONDUCTOR_WORKER_ALL_POLL_INTERVAL` | Default poll interval (ms) |
| `CONDUCTOR_WORKER_ALL_THREAD_COUNT` | Default concurrent executions |
| `CONDUCTOR_WORKER_ALL_DOMAIN` | Default task domain |
| `CONDUCTOR_WORKER_{NAME}_POLL_INTERVAL` | Worker-specific poll interval |
| `CONDUCTOR_WORKER_{NAME}_THREAD_COUNT` | Worker-specific thread count |
| `CONDUCTOR_WORKER_{NAME}_DOMAIN` | Worker-specific domain |

---

## Client APIs

### ConductorClient

The main entry point providing access to all API clients.

```rust
use conductor::{ConductorClient, Configuration};

let client = ConductorClient::new(Configuration::from_env())?;

// Access specialized clients
let task_client = client.task_client();
let workflow_client = client.workflow_client();
let metadata_client = client.metadata_client();
```

### TaskClient

Operations for task polling and updates.

| Method | Description |
|--------|-------------|
| `poll_task()` | Poll for a single task |
| `batch_poll()` | Poll for multiple tasks |
| `update_task()` | Update task result |
| `update_task_with_retry()` | Update with automatic retry |
| `get_task()` | Get task by ID |
| `get_tasks_in_progress()` | Get running tasks for a type |
| `add_task_log()` | Add execution log |
| `get_task_logs()` | Get task logs |
| `get_queue_sizes()` | Get queue depths |
| `remove_task_from_queue()` | Remove task from queue |

```rust
// Batch poll for tasks
let tasks = task_client
    .batch_poll("my_task", Some("worker-1"), None, 10, Duration::from_secs(1))
    .await?;

// Update task with retry
let result = TaskResult::completed(&task.task_id, &task.workflow_instance_id)
    .with_output_value("result", "done");
task_client.update_task_with_retry(&result, 3).await?;
```

### WorkflowClient

Operations for workflow execution and lifecycle management.

| Method | Description |
|--------|-------------|
| `start_workflow()` | Start workflow asynchronously |
| `execute_workflow()` | Start and wait for completion |
| `get_workflow()` | Get workflow by ID |
| `get_workflow_status()` | Get status with optional output |
| `terminate_workflow()` | Terminate running workflow |
| `pause_workflow()` | Pause workflow |
| `resume_workflow()` | Resume paused workflow |
| `retry_workflow()` | Retry failed workflow |
| `restart_workflow()` | Restart from beginning |
| `rerun_workflow()` | Rerun from specific task |
| `update_variables()` | Update workflow variables |
| `skip_task()` | Skip a task in workflow |
| `search_workflows()` | Search workflows |
| `get_running_workflows()` | Get running workflow IDs |
| `delete_workflow()` | Delete workflow execution |
| `test_workflow()` | Dry run with mocked outputs |

```rust
use conductor::models::StartWorkflowRequest;

// Start workflow
let request = StartWorkflowRequest::new("my_workflow")
    .with_version(1)
    .with_input_value("name", "John");

let workflow_id = workflow_client.start_workflow(&request).await?;

// Execute and wait
let workflow = workflow_client
    .execute_workflow(&request, Duration::from_secs(30))
    .await?;
```

### MetadataClient

Operations for managing workflow and task definitions.

| Method | Description |
|--------|-------------|
| `register_workflow_def()` | Register workflow definition |
| `update_workflow_def()` | Update workflow definition |
| `register_or_update_workflow_def()` | Upsert workflow definition |
| `get_workflow_def()` | Get workflow definition |
| `get_all_workflow_def_versions()` | Get all versions |
| `get_all_workflow_defs()` | List all definitions |
| `delete_workflow_def()` | Delete workflow definition |
| `register_task_def()` | Register task definition |
| `register_task_defs()` | Register multiple tasks |
| `update_task_def()` | Update task definition |
| `get_task_def()` | Get task definition |
| `get_all_task_defs()` | List all task definitions |
| `delete_task_def()` | Delete task definition |
| `task_def_exists()` | Check if task exists |
| `workflow_def_exists()` | Check if workflow exists |

```rust
use conductor::models::{TaskDef, WorkflowDef, WorkflowTask, TimeoutPolicy};

// Register task definition
let task_def = TaskDef::new("my_task")
    .with_description("My task")
    .with_retry(3, RetryLogic::Fixed, 10)
    .with_timeout(60, TimeoutPolicy::TimeOutWf);

metadata_client.register_task_def(&task_def).await?;

// Register workflow definition
let workflow_def = WorkflowDef::new("my_workflow")
    .with_version(1)
    .with_task(WorkflowTask::simple("my_task", "task_ref_1"));

metadata_client.register_workflow_def(&workflow_def).await?;
```

---

## Worker Framework

### Worker Trait

The core trait for implementing task workers.

```rust
use async_trait::async_trait;
use conductor::{Worker, Task, error::Result, worker::WorkerOutput};

#[async_trait]
pub trait Worker: Send + Sync {
    /// Task type this worker handles
    fn task_definition_name(&self) -> &str;
    
    /// Execute the task
    async fn execute(&self, task: &Task) -> Result<WorkerOutput>;
    
    // Optional overrides with defaults
    fn identity(&self) -> String { /* hostname-pid */ }
    fn domain(&self) -> Option<&str> { None }
    fn poll_interval_millis(&self) -> u64 { 100 }
    fn thread_count(&self) -> usize { 1 }
}
```

### WorkerOutput

```rust
pub enum WorkerOutput {
    /// Task completed with output data
    Completed(HashMap<String, Value>),
    
    /// Task failed with error message
    Failed(String),
    
    /// Task still in progress (for long-running tasks)
    InProgress(TaskInProgress),
}

// Convenience constructors
WorkerOutput::completed_with_result("result_value")
WorkerOutput::completed(hashmap!{"key" => "value"})
WorkerOutput::failed("Error message")
WorkerOutput::in_progress(60)  // callback after 60 seconds
```

### FnWorker

Function-based worker for simple use cases.

```rust
use conductor::worker::FnWorker;

let worker = FnWorker::new("my_task", |task: Task| async move {
    let input = task.get_input_string("name").unwrap_or_default();
    Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", input)))
})
.with_thread_count(4)
.with_poll_interval_millis(500)
.with_domain("my-domain");
```

### #[worker] Macro (Optional)

For a more declarative style similar to Python's `@worker_task` decorator, enable the `macros` feature:

```toml
[dependencies]
conductor = { version = "0.1", package = "conductor-sdk", features = ["macros"] }
```

Then use the `#[worker]` attribute macro:

```rust
use conductor_macros::worker;

// Simple worker with automatic parameter extraction
#[worker(name = "greet", thread_count = 5)]
async fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

// Worker with multiple parameters
#[worker(name = "calculate", poll_interval = 200)]
async fn calculate(a: i32, b: i32) -> serde_json::Value {
    serde_json::json!({ "sum": a + b })
}

// Worker with full Task access
#[worker(name = "process", domain = "premium")]
async fn process(task: Task) -> WorkerOutput {
    let data = task.get_input_string("data").unwrap();
    WorkerOutput::completed_with_result(data)
}

// Worker with Result return type
#[worker(name = "validate")]
async fn validate(value: String) -> Result<String, String> {
    if value.is_empty() {
        Err("Value cannot be empty".to_string())
    } else {
        Ok(format!("Valid: {}", value))
    }
}

// Usage - macro generates {fn_name}_worker() functions
let mut handler = TaskHandler::new(config)?;
handler.add_worker(greet_worker());
handler.add_worker(calculate_worker());
handler.add_worker(process_worker());
handler.add_worker(validate_worker());
```

### JSON Schema Generation

Generate JSON Schemas from Rust types for input/output validation:

```rust
use conductor::schema::generate_schema;
use schemars::JsonSchema;

#[derive(JsonSchema)]
struct OrderInput {
    order_id: String,
    amount: f64,
    items: Vec<String>,
}

#[derive(JsonSchema)]
struct OrderOutput {
    status: String,
    processed_at: String,
}

// Generate schema with strict mode (additionalProperties: false)
let input_schema = generate_schema::<OrderInput>(true);
let output_schema = generate_schema::<OrderOutput>(true);

// Attach schemas to worker - auto-registered when register_task_def=true
let worker = FnWorker::new("process_order", handler)
    .with_input_schema(input_schema)
    .with_output_schema(output_schema);

// Or use the convenience methods
let worker = FnWorker::new("process_order", handler)
    .with_input_schema_from::<OrderInput>(true)
    .with_output_schema_from::<OrderOutput>(true);
```

### TaskContext

Access task metadata including poll_count for long-running tasks:

```rust
use conductor::worker::TaskContext;

let worker = FnWorker::new("long_task", |task: Task| async move {
    let ctx = TaskContext::from_task(&task);
    
    // Access metadata
    println!("Poll count: {}", ctx.poll_count());
    println!("Retry count: {}", ctx.retry_count());
    println!("Task ID: {}", ctx.task_id());
    println!("Workflow ID: {}", ctx.workflow_instance_id());
    
    // Convenience methods
    if ctx.is_first_poll() {
        // Initialize task
    }
    if ctx.is_retry() {
        // Handle retry differently
    }
    
    if ctx.poll_count() < 5 {
        Ok(WorkerOutput::in_progress(30))
    } else {
        Ok(WorkerOutput::completed_with_result("done"))
    }
});
```

### TaskHandler

Manages multiple workers with lifecycle control.

```rust
use conductor::{Configuration, worker::TaskHandler};

let mut handler = TaskHandler::builder(Configuration::from_env())
    .worker(worker1)
    .worker(worker2)
    .metrics(MetricsSettings::default().with_http_port(9090))
    .build()?;

handler.start().await?;

// Control individual workers
handler.pause_worker("task_type_1");
handler.resume_worker("task_type_1");

// Control all workers
handler.pause_all();
handler.resume_all();

// Graceful shutdown
handler.stop().await?;
```

### WorkerHost

High-level API with automatic shutdown handling.

```rust
use conductor::{WorkerHost, Configuration};

let host = WorkerHost::builder(Configuration::from_env())
    .worker(my_worker)
    .with_metrics(MetricsSettings::default().with_http_port(9090))
    .start()
    .await?;

// Runs until Ctrl+C
host.wait_for_shutdown().await?;
```

### Programmatic Shutdown

```rust
let (builder, shutdown_tx) = WorkerHost::builder(config)
    .worker(my_worker)
    .with_shutdown_channel();

let host = builder.start().await?;

// Later, trigger shutdown programmatically
shutdown_tx.send(()).ok();
```

---

## Models

### Task Models

| Type | Description |
|------|-------------|
| `Task` | Task instance with input/output data |
| `TaskStatus` | `Scheduled`, `InProgress`, `Completed`, `Failed`, `FailedWithTerminalError`, `Canceled`, `Skipped`, `TimedOut` |
| `TaskExecLog` | Task execution log entry |
| `TaskResult` | Result submitted back to Conductor |
| `TaskResultStatus` | `Completed`, `Failed`, `FailedWithTerminalError`, `InProgress` |
| `TaskInProgress` | Marker for long-running tasks |
| `TaskDef` | Task definition/metadata |
| `RetryLogic` | `Fixed`, `ExponentialBackoff`, `LinearBackoff` |
| `TimeoutPolicy` | `Retry`, `TimeOutWf`, `AlertOnly` |

### Workflow Models

| Type | Description |
|------|-------------|
| `Workflow` | Workflow execution instance |
| `WorkflowStatus` | `Running`, `Completed`, `Failed`, `TimedOut`, `Terminated`, `Paused` |
| `StartWorkflowRequest` | Request to start a workflow |
| `WorkflowDef` | Workflow definition |
| `WorkflowTask` | Task within a workflow definition |
| `TaskType` | `Simple`, `SubWorkflow`, `Fork`, `Join`, `Switch`, `DoWhile`, `Http`, `Inline`, `HttpPoll`, `WaitForWebhook`, `LlmTextComplete`, `LlmChatComplete`, `LlmGenerateEmbeddings`, `LlmSearchIndex`, `LlmIndexText`, `LlmIndexDocument`, `LlmGetEmbeddings`, `GetDocument`, etc. |
| `SubWorkflowParams` | Parameters for subworkflow tasks |
| `WorkflowTimeoutPolicy` | `TimeOutWf`, `AlertOnly` |
| `StateChangeConfig` | Configuration for task state change events |
| `StateChangeEvent` | Event dispatched on task state transitions |
| `ChatMessage` | Message for LLM chat completions |

### Builder Pattern

All models support fluent builder patterns:

```rust
// Task Definition
let task_def = TaskDef::new("process_order")
    .with_description("Process customer orders")
    .with_retry(3, RetryLogic::ExponentialBackoff, 10)
    .with_timeout(300, TimeoutPolicy::TimeOutWf)
    .with_rate_limit(100, 60)
    .with_owner("team@example.com");

// Workflow Definition
let workflow_def = WorkflowDef::new("order_workflow")
    .with_version(1)
    .with_description("Order processing workflow")
    .with_task(WorkflowTask::simple("validate_order", "validate_ref"))
    .with_task(WorkflowTask::simple("process_order", "process_ref"))
    .with_task(WorkflowTask::simple("notify_customer", "notify_ref"))
    .with_output_param("orderId", "${process_ref.output.orderId}")
    .with_timeout(3600, WorkflowTimeoutPolicy::TimeOutWf);

// Workflow Task Variants
WorkflowTask::simple("task_name", "reference_name")
WorkflowTask::sub_workflow("ref_name", "child_workflow")
WorkflowTask::http("ref_name", "https://api.example.com/endpoint")
WorkflowTask::inline("ref_name", "function e() { return $.input * 2; }")
WorkflowTask::wait("wait_ref")
```

---

## LLM/AI Tasks (Orkes)

The SDK supports Orkes AI/LLM tasks for building AI-powered workflows. These tasks integrate with various LLM providers (OpenAI, Azure, Anthropic, etc.) through Orkes' AI integration layer.

### LLM Task Types

| Task Type | Description |
|-----------|-------------|
| `LlmTextComplete` | Text completion using an LLM model |
| `LlmChatComplete` | Chat completion with conversation context |
| `LlmGenerateEmbeddings` | Generate vector embeddings from text |
| `LlmSearchIndex` | Search a vector database index |
| `LlmIndexText` | Index text into a vector database |
| `LlmIndexDocument` | Index a document into a vector database |
| `LlmGetEmbeddings` | Query existing embeddings |
| `GetDocument` | Fetch document content from URL |

### LLM Text Completion

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("text_completion_workflow")
    .with_task(
        WorkflowTask::llm_text_complete("completion_ref", "my_llm_provider", "gpt-4")
            .with_instructions_template("Summarize the following text: ${workflow.input.text}")
            .with_temperature(0.7)
            .with_max_tokens(500)
    );
```

### LLM Chat Completion

```rust
use conductor::models::{WorkflowDef, WorkflowTask, ChatMessage};

let workflow = WorkflowDef::new("chat_workflow")
    .with_task(
        WorkflowTask::llm_chat_complete("chat_ref", "my_llm_provider", "gpt-4")
            .with_messages(vec![
                ChatMessage::system("You are a helpful assistant."),
                ChatMessage::user("${workflow.input.question}"),
            ])
            .with_temperature(0.8)
            .with_max_tokens(1000)
    );
```

### Vector Database Operations (RAG)

```rust
// Generate embeddings
let embed_task = WorkflowTask::llm_generate_embeddings(
    "embed_ref", 
    "my_embedding_provider", 
    "text-embedding-ada-002"
)
.with_input_param("text", "${workflow.input.document}");

// Index text to vector DB
let index_task = WorkflowTask::llm_index_text(
    "index_ref",
    "my_embedding_provider",
    "text-embedding-ada-002",
    "my_vector_db",
    "my_index"
)
.with_namespace("documents")
.with_input_param("text", "${workflow.input.document}");

// Search vector DB
let search_task = WorkflowTask::llm_search_index(
    "search_ref",
    "my_embedding_provider", 
    "text-embedding-ada-002",
    "my_vector_db",
    "my_index"
)
.with_namespace("documents")
.with_max_results(5)
.with_input_param("query", "${workflow.input.query}");
```

### Prompt Variables

Use prompt variables for dynamic prompt templates:

```rust
let task = WorkflowTask::llm_text_complete("ref", "provider", "model")
    .with_instructions_template("Translate to ${target_language}: ${text}")
    .with_prompt_variable("target_language", "${workflow.input.language}")
    .with_prompt_variable("text", "${workflow.input.content}");
```

---

## HTTP Poll Task

The HTTP Poll task repeatedly calls an HTTP endpoint until a termination condition is met.

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("polling_workflow")
    .with_task(
        WorkflowTask::http_poll("poll_ref", "https://api.example.com/status/${workflow.input.job_id}")
            .with_method("GET")
            .with_polling_interval(5)  // Poll every 5 seconds
            .with_polling_strategy("FIXED")
            .with_termination_condition("(function(){ return $.output.body.status === 'COMPLETED'; })();")
    );
```

### HTTP Poll Parameters

| Parameter | Description |
|-----------|-------------|
| `polling_interval` | Milliseconds between polls |
| `polling_strategy` | `FIXED` or `EXPONENTIAL_BACKOFF` |
| `termination_condition` | JavaScript expression that returns true when polling should stop |

---

## Wait for Webhook Task

The Wait for Webhook task pauses workflow execution until an external webhook is received.

```rust
use conductor::models::{WorkflowDef, WorkflowTask};
use serde_json::json;

let workflow = WorkflowDef::new("webhook_workflow")
    .with_task(
        WorkflowTask::wait_for_webhook("webhook_ref")
            .with_matches(json!({
                "type": "payment_received",
                "order_id": "${workflow.input.order_id}"
            }))
    );
```

When a webhook is received that matches the criteria, the workflow continues with the webhook payload as the task output.

---

## State Change Events

Configure tasks to emit events on state transitions for audit logging, monitoring, or triggering external systems.

### Event Types

| Event Type | Description |
|------------|-------------|
| `OnScheduled` | Task is scheduled for execution |
| `OnStart` | Task execution begins |
| `OnCompleted` | Task completes successfully |
| `OnFailed` | Task fails |
| `OnCancelled` | Task is cancelled |

### Configuration

```rust
use conductor::models::{WorkflowTask, StateChangeConfig, StateChangeEvent, StateChangeEventType};

let task = WorkflowTask::simple("process_order", "process_ref")
    .with_state_change(
        StateChangeConfig::new()
            .with_event(StateChangeEvent::new(StateChangeEventType::OnStart)
                .with_type("task:audit")
                .with_payload(json!({
                    "task": "${task.taskDefName}",
                    "workflow": "${workflow.workflowType}",
                    "timestamp": "${task.startTime}"
                })))
            .with_event(StateChangeEvent::new(StateChangeEventType::OnCompleted)
                .with_type("task:audit")
                .with_payload(json!({
                    "task": "${task.taskDefName}",
                    "status": "COMPLETED",
                    "duration_ms": "${task.endTime - task.startTime}"
                })))
            .with_event(StateChangeEvent::new(StateChangeEventType::OnFailed)
                .with_type("task:audit")
                .with_payload(json!({
                    "task": "${task.taskDefName}",
                    "status": "FAILED",
                    "error": "${task.reasonForIncompletion}"
                })))
    );
```

---

## Fork/Join with Script

Customize join behavior using a JavaScript evaluator script:

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

// Fork with dynamic join completion
let fork_tasks = vec![
    vec![WorkflowTask::simple("task_a", "task_a_ref")],
    vec![WorkflowTask::simple("task_b", "task_b_ref")],
    vec![WorkflowTask::simple("task_c", "task_c_ref")],
];

let workflow = WorkflowDef::new("fork_join_script_workflow")
    .with_task(WorkflowTask::fork_join("fork_ref", fork_tasks, "join_ref"))
    .with_task(
        WorkflowTask::join("join_ref", vec!["task_a_ref", "task_b_ref", "task_c_ref"])
            .with_join_script("(function(){ return $.joinOn.length >= 2; })();")
    );
```

The join script receives `$.joinOn` containing completed task references and returns `true` when enough tasks have completed.

---

## Events System

### Event Types

| Event | Description | Fields |
|-------|-------------|--------|
| `PollStarted` | Polling begins | `task_type`, `worker_id`, `poll_count` |
| `PollCompleted` | Polling succeeds | `task_type`, `worker_id`, `duration`, `tasks_received` |
| `PollFailure` | Polling fails | `task_type`, `worker_id`, `duration`, `error` |
| `TaskExecutionStarted` | Task execution begins | `task_type`, `task_id`, `workflow_instance_id`, `worker_id` |
| `TaskExecutionCompleted` | Task execution succeeds | `task_type`, `task_id`, `duration`, `output_size_bytes` |
| `TaskExecutionFailure` | Task execution fails | `task_type`, `task_id`, `duration`, `error`, `is_retryable` |
| `TaskUpdateFailure` | Task update fails | `task_type`, `task_id`, `error`, `retry_count` |

### Event Listener

```rust
use conductor::events::{TaskRunnerEventsListener, PollCompleted, TaskExecutionCompleted};

struct MyListener;

impl TaskRunnerEventsListener for MyListener {
    fn on_poll_completed(&self, event: &PollCompleted) {
        println!("Polled {} tasks in {:?}", event.tasks_received, event.duration);
    }
    
    fn on_task_execution_completed(&self, event: &TaskExecutionCompleted) {
        println!("Task {} completed in {:?}", event.task_id, event.duration);
    }
}

// Register listener
handler.add_event_listener(Arc::new(MyListener));
```

### Event Flow

```
┌──────────────┐    ┌──────────────┐    ┌──────────────────┐
│  TaskRunner  │───▶│EventDispatcher│───▶│ Listener 1       │
│              │    │              │    │ (MetricsCollector)│
│ • poll       │    │ • publish_*  │    └──────────────────┘
│ • execute    │    │              │    ┌──────────────────┐
│ • update     │    │              │───▶│ Listener 2       │
└──────────────┘    └──────────────┘    │ (Custom Logger)  │
                                        └──────────────────┘
```

---

## Metrics

The `MetricsCollector` in `src/metrics/collector.rs` implements the full
canonical Prometheus catalog using the `prometheus` crate. It is wired into
the worker framework via `TaskHandler::enable_metrics`, which registers it as
both a `TaskRunnerEventsListener` (for task-level events) and as the
`HttpMetricsObserver` on `ApiClient` (for HTTP request latency). An optional
built-in HTTP server exposes `/metrics` and `/health` endpoints for scraping.

```rust
use conductor::metrics::MetricsSettings;

handler.enable_metrics(
    MetricsSettings::new()
        .with_http_port(9090)
        .with_metrics_path("/metrics"),
);
```

See [METRICS.md](METRICS.md) for the complete metric catalog, labels, bucket
sets, intentional divergences, and example scrape output.

---

## Error Handling

### ConductorError

```rust
pub enum ConductorError {
    Http(reqwest::Error),           // HTTP request failed
    Json(serde_json::Error),        // Serialization failed
    Config(String),                  // Configuration error
    Auth(String),                    // Authentication error
    TaskExecution(String),           // Task execution error
    TaskNotFound(String),            // Task not found
    WorkflowNotFound(String),        // Workflow not found
    Workflow(String),                // Workflow error
    Worker(String),                  // Worker error
    Timeout(String),                 // Timeout error
    Server { status, message },      // Server error (5xx)
    Api { message, code },           // API error
    Internal(String),                // Internal error
    Io(std::io::Error),             // IO error
    Channel(String),                 // Channel error
}
```

### Error Handling Pattern

```rust
use conductor::error::{ConductorError, Result};

async fn process_task(task: &Task) -> Result<WorkerOutput> {
    // Errors automatically converted via From trait
    let data: MyData = task.get_input("data")
        .ok_or_else(|| ConductorError::task_execution("Missing 'data' input"))?;
    
    // Check if error is retryable
    match some_operation().await {
        Err(e) if e.is_retryable() => {
            // Return in-progress to retry later
            Ok(WorkerOutput::in_progress(30))
        }
        Err(e) => Err(e),
        Ok(result) => Ok(WorkerOutput::completed_with_result(result)),
    }
}
```

### Result Type

```rust
pub type Result<T> = std::result::Result<T, ConductorError>;
```

---

## Examples

The SDK includes 26 examples covering all major features. Run examples with:

```bash
cargo run --example <example_name>
cargo run --example worker_macro_example --features macros
```

### Available Examples

| Example | Description |
|---------|-------------|
| `hello_world` | Basic workflow with simple worker |
| `async_workers` | Multiple concurrent workers |
| `dynamic_workflow` | Build workflows programmatically |
| `http_task_example` | HTTP task integration |
| `sub_workflow_example` | Sub-workflow composition |
| `fork_join_example` | Parallel execution with fork/join |
| `switch_task_example` | Conditional branching |
| `do_while_example` | Loop constructs |
| `terminate_example` | Workflow termination |
| `set_variable_example` | Workflow variables |
| `inline_task_example` | Inline JavaScript tasks |
| `wait_task_example` | Wait/human tasks |
| `event_driven_example` | Event-driven workers |
| `metrics_example` | Prometheus metrics |
| `json_schema_example` | Input/output validation |
| `worker_macro_example` | `#[worker]` macro (requires `macros` feature) |
| `openai_helloworld` | LLM text completion |
| `llm_chat_example` | LLM chat with prompts |
| `vector_db_example` | RAG with vector DB |
| `http_poll_example` | HTTP polling task |
| `wait_for_webhook_example` | Webhook integration |
| `sync_state_update_example` | Synchronous state updates |
| `workflow_rerun_example` | Rerun from specific task |
| `fork_join_script_example` | Fork/join with completion script |
| `task_status_audit_example` | State change audit events |
| `dynamic_task_example` | Dynamic task routing |

### Hello World

```rust
use conductor::{
    Configuration, WorkerHost,
    models::{StartWorkflowRequest, WorkflowDef, WorkflowTask, TaskDef},
    worker::{FnWorker, WorkerOutput},
};

#[tokio::main]
async fn main() -> conductor::error::Result<()> {
    let config = Configuration::from_env();
    let client = conductor::ConductorClient::new(config.clone())?;
    
    // Register task
    client.metadata_client()
        .register_task_def(&TaskDef::new("greet"))
        .await?;
    
    // Register workflow
    client.metadata_client()
        .register_workflow_def(
            &WorkflowDef::new("greeting_workflow")
                .with_task(WorkflowTask::simple("greet", "greet_ref"))
        )
        .await?;
    
    // Create worker
    let worker = FnWorker::new("greet", |task| async move {
        let name = task.get_input_string("name").unwrap_or_default();
        Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", name)))
    });
    
    // Start worker host
    let host = WorkerHost::builder(config)
        .worker(worker)
        .start()
        .await?;
    
    // Start a workflow
    let workflow_id = client.workflow_client()
        .start_workflow(
            &StartWorkflowRequest::new("greeting_workflow")
                .with_input_value("name", "World")
        )
        .await?;
    
    println!("Started workflow: {}", workflow_id);
    
    host.wait_for_shutdown().await
}
```

### Multiple Workers with Metrics

```rust
use conductor::{
    Configuration, MetricsSettings,
    worker::{FnWorker, TaskHandler, WorkerOutput},
};

#[tokio::main]
async fn main() -> conductor::error::Result<()> {
    let config = Configuration::from_env();
    
    let mut handler = TaskHandler::builder(config)
        .worker(FnWorker::new("task_a", |_| async {
            Ok(WorkerOutput::completed_with_result("A done"))
        }).with_thread_count(4))
        .worker(FnWorker::new("task_b", |_| async {
            Ok(WorkerOutput::completed_with_result("B done"))
        }).with_thread_count(2))
        .metrics(MetricsSettings::default().with_http_port(9090))
        .build()?;
    
    handler.start().await?;
    
    // Metrics available at http://localhost:9090/metrics
    
    tokio::signal::ctrl_c().await?;
    handler.stop().await
}
```

### Long-Running Task

```rust
FnWorker::new("long_task", |task| async move {
    let progress: i32 = task.get_input("progress").unwrap_or(0);
    
    if progress < 100 {
        // Report progress and callback in 10 seconds
        Ok(WorkerOutput::InProgress(
            TaskInProgress::new(10)
                .with_output_value("progress", progress + 10)
                .with_output_value("status", "Processing...")
        ))
    } else {
        Ok(WorkerOutput::completed_with_result("Complete!"))
    }
})
```

### Custom Worker Implementation

```rust
use async_trait::async_trait;
use conductor::{Worker, Task, error::Result, worker::WorkerOutput};

struct OrderProcessor {
    db_pool: DatabasePool,
}

#[async_trait]
impl Worker for OrderProcessor {
    fn task_definition_name(&self) -> &str {
        "process_order"
    }
    
    fn thread_count(&self) -> usize {
        8
    }
    
    fn poll_interval_millis(&self) -> u64 {
        200
    }
    
    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        let order_id: String = task.get_input("orderId")
            .ok_or_else(|| ConductorError::task_execution("Missing orderId"))?;
        
        // Process order...
        let result = self.db_pool.process(&order_id).await?;
        
        Ok(WorkerOutput::completed_with_result(result))
    }
}
```

---

## Comparison with Python SDK

| Feature | Rust SDK | Python SDK |
|---------|----------|------------|
| Async Runtime | Tokio | asyncio |
| Type Safety | Compile-time | Runtime |
| Worker Trait | `async_trait` | `abc.ABC` |
| Configuration | `Configuration` | `Configuration` |
| Environment Variables | Same naming | Same naming |
| Event System | `EventDispatcher` | `EventDispatcher` |
| Metrics | Prometheus | Prometheus |
| Batch Polling | Yes | Yes |
| Auto Retry | Yes | Yes |

---

## Thread Safety

All public types in this SDK are designed to be `Send + Sync`:

- `ConductorClient`, `TaskClient`, `WorkflowClient`, `MetadataClient` - All cloneable and thread-safe
- `TaskHandler` - Thread-safe, workers can be paused/resumed from any thread
- `EventDispatcher` - Thread-safe registration and publishing
- `MetricsCollector` - Thread-safe metric updates

Workers are wrapped in `Arc<dyn Worker>` and can be safely shared across threads.

---

## Performance Considerations

1. **Batch Polling**: Use batch polling to reduce HTTP overhead when processing many tasks
2. **Thread Count**: Set appropriate `thread_count` per worker based on task complexity
3. **Poll Interval**: Balance between responsiveness and server load
4. **Connection Pooling**: The HTTP client maintains a connection pool internally
5. **Semaphore-based Concurrency**: Task execution is controlled by semaphores to prevent overload

---

## License

This SDK is provided under the same license as the Conductor project.
