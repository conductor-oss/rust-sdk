# Rust SDK for Conductor

[![CI](https://github.com/conductor-oss/conductor-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/conductor-oss/conductor-rust/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/conductor-rust.svg)](https://crates.io/crates/conductor-rust)
[![Rust Versions](https://img.shields.io/badge/rust-1.70%2B-blue.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/conductor-rust.svg)](LICENSE)

Rust SDK for [Conductor](https://www.conductor-oss.org/) (OSS and Orkes Conductor) — an orchestration platform for building distributed applications, AI agents, and workflow-driven microservices. Define workflows as code, run workers anywhere, and let Conductor handle retries, state management, and observability.

If you find [Conductor](https://github.com/conductor-oss/conductor) useful, please consider giving it a star on GitHub -- it helps the project grow.

[![GitHub stars](https://img.shields.io/github/stars/conductor-oss/conductor.svg?style=social&label=Star&maxAge=)](https://GitHub.com/conductor-oss/conductor/)

<!-- TOC -->
* [Rust SDK for Conductor](#rust-sdk-for-conductor)
  * [Start Conductor server](#start-conductor-server)
  * [Install the SDK](#install-the-sdk)
  * [60-Second Quickstart](#60-second-quickstart)
  * [Comprehensive worker example](#comprehensive-worker-example)
  * [Workers](#workers)
  * [Monitoring Workers](#monitoring-workers)
  * [Workflows](#workflows)
  * [Troubleshooting](#troubleshooting)
  * [AI & LLM Workflows](#ai--llm-workflows)
  * [Examples](#examples)
  * [API Journey Examples](#api-journey-examples)
  * [Documentation](#documentation)
  * [Support](#support)
  * [Frequently Asked Questions](#frequently-asked-questions)
  * [License](#license)
<!-- TOC -->

## Start Conductor server

If you don't already have a Conductor server running, pick one:

**Docker Compose (recommended, includes UI):**

```shell
docker run -p 8080:8080 conductoross/conductor:latest
```
The UI will be available at `http://localhost:8080` and the API at `http://localhost:8080/api`

**MacOS / Linux (one-liner):** (If you don't want to use docker, you can install and run the binary directly)
```shell
curl -sSL https://raw.githubusercontent.com/conductor-oss/conductor/main/conductor_server.sh | sh
```

**Conductor CLI**
```shell
# Installs conductor cli
npm install -g @conductor-oss/conductor-cli

# Start the open source conductor server
conductor server start
# see conductor server --help for all the available commands
```

## Install the SDK

Add the following to your `Cargo.toml`:

```toml
[dependencies]
conductor = "0.1"
tokio = { version = "1", features = ["full"] }
```

For the `#[worker]` macro (similar to Python's `@worker_task` decorator):

```toml
[dependencies]
conductor = { version = "0.1", features = ["macros"] }
conductor-macros = "0.1"
tokio = { version = "1", features = ["full"] }
```

## 60-Second Quickstart

**Step 1: Create a workflow**

Workflows are definitions that reference task types (e.g. a SIMPLE task called `greet`). We'll build a workflow called
`greetings` that runs one task and returns its output.

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

fn greetings_workflow() -> WorkflowDef {
    WorkflowDef::new("greetings")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("greet", "greet_ref")
                .with_input_param("name", "${workflow.input.name}")
        )
        .with_output_param("result", "${greet_ref.output.result}")
}
```

**Step 2: Write worker**

Workers are Rust functions decorated with `#[worker]` that poll Conductor for tasks and execute them.

```rust
use conductor_macros::worker;

#[worker(name = "greet")]
async fn greet(name: String) -> String {
    format!("Hello {}", name)
}
```

**Step 3: Run your first workflow app**

Create a `main.rs` with the following:

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{StartWorkflowRequest, WorkflowDef, WorkflowTask},
    worker::TaskHandler,
};
use conductor_macros::worker;

// A worker is any Rust function with the #[worker] macro.
#[worker(name = "greet")]
async fn greet(name: String) -> String {
    format!("Hello {}", name)
}

fn greetings_workflow() -> WorkflowDef {
    WorkflowDef::new("greetings")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("greet", "greet_ref")
                .with_input_param("name", "${workflow.input.name}")
        )
        .with_output_param("result", "${greet_ref.output.result}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure the SDK (reads CONDUCTOR_SERVER_URL / CONDUCTOR_AUTH_* from env).
    let config = Configuration::default();
    let client = ConductorClient::new(config.clone())?;

    // Register the workflow
    let workflow = greetings_workflow();
    client.metadata_client()
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    // Start polling for tasks
    let mut task_handler = TaskHandler::new(config.clone())?;
    task_handler.add_worker(greet_worker());
    task_handler.start().await?;

    // Run the workflow and get the result
    let run = client.workflow_client()
        .execute_workflow(
            &StartWorkflowRequest::new("greetings")
                .with_version(1)
                .with_input_value("name", "Conductor"),
            std::time::Duration::from_secs(10),
        )
        .await?;

    println!("result: {:?}", run.output.get("result"));
    println!("execution: {}/execution/{}", config.ui_host, run.workflow_id);

    task_handler.stop().await?;
    Ok(())
}
```

Run it:

```shell
cargo run
```

> ### Using Orkes Conductor / Remote Server?
> Export your authentication credentials as well:
>
> ```shell
> export CONDUCTOR_SERVER_URL="https://your-cluster.orkesconductor.io/api"
>
> # If using Orkes Conductor that requires auth key/secret
> export CONDUCTOR_AUTH_KEY="your-key"
> export CONDUCTOR_AUTH_SECRET="your-secret"
> ```
> See [Configuration](#configuration) for details.

That's it -- you just defined a worker, built a workflow, and executed it. Open the Conductor UI (default:
[http://localhost:8080](http://localhost:8080)) to see the execution.

## Comprehensive worker example

The example includes sync + async workers, metrics, and long-running tasks.

See [examples/worker_example.rs](examples/worker_example.rs)

---

## Workers

Workers are Rust functions that execute Conductor tasks. Use the `#[worker]` macro or `FnWorker` to:

- register it as a worker (auto-discovered by `TaskHandler`)
- use it as a workflow task (call it with `task_ref_name=...`)

Note: Workers can also be used by LLMs for tool calling (see [AI & LLM Workflows](#ai--llm-workflows)).

```rust
use conductor_macros::worker;

#[worker(name = "greet")]
async fn greet(name: String) -> String {
    format!("Hello {}", name)
}
```

**Using FnWorker (closure-based):**

```rust
use conductor::worker::{FnWorker, WorkerOutput};

let greetings_worker = FnWorker::new("greetings", |task| async move {
    let name = task.get_input_string("name").unwrap_or_default();
    Ok(WorkerOutput::completed_with_result(format!("Hello, {}", name)))
})
.with_thread_count(10)
.with_poll_interval_millis(100);
```

**Start workers** with `TaskHandler`:

```rust
use conductor::{
    configuration::Configuration,
    worker::TaskHandler,
};

let config = Configuration::default();
let mut task_handler = TaskHandler::new(config)?;
task_handler.add_worker(greet_worker());

task_handler.start().await?;

// Wait for shutdown signal
tokio::signal::ctrl_c().await?;

task_handler.stop().await?;
```

**Worker Configuration**

Workers support hierarchical environment variable configuration — global settings that can be overridden per worker:

```shell
# Global (all workers)
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=20
export CONDUCTOR_WORKER_ALL_DOMAIN=production

# Per-worker override
export CONDUCTOR_WORKER_GREETINGS_THREAD_COUNT=50
```

See [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md) for all options.

## Monitoring Workers

Enable Prometheus metrics:

```rust
use conductor::metrics::MetricsSettings;
use conductor::worker::TaskHandler;

let mut task_handler = TaskHandler::new(config)?;
task_handler.enable_metrics(
    MetricsSettings::new()
        .with_http_port(9090)
);

task_handler.start().await?;
// Metrics at http://localhost:9090/metrics
```

See [METRICS.md](METRICS.md) for details.

**Learn more:**
- [Worker Guide](docs/WORKER.md) — All worker patterns (function, closure, macro, async)
- [Worker Configuration](WORKER_CONFIGURATION.md) — Environment variable configuration system

## Workflows

Define workflows in Rust using the builder pattern to chain tasks:

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{WorkflowDef, WorkflowTask},
};

let config = Configuration::default();
let client = ConductorClient::new(config)?;
let metadata_client = client.metadata_client();

let workflow = WorkflowDef::new("greetings")
    .with_version(1)
    .with_task(
        WorkflowTask::simple("greet", "greet_ref")
            .with_input_param("name", "${workflow.input.name}")
    )
    .with_output_param("result", "${greet_ref.output.result}");

// Registering is required if you want to start/execute by name+version
metadata_client.register_or_update_workflow_def(&workflow, true).await?;
```

**Execute workflows:**

```rust
use conductor::models::StartWorkflowRequest;
use std::time::Duration;

// Asynchronous (returns workflow ID immediately)
let request = StartWorkflowRequest::new("greetings")
    .with_version(1)
    .with_input_value("name", "Orkes");
let workflow_id = workflow_client.start_workflow(&request).await?;

// Synchronous (waits for completion)
let run = workflow_client
    .execute_workflow(&request, Duration::from_secs(10))
    .await?;
println!("{:?}", run.output);
```

**Manage running workflows and send signals:**

```rust
workflow_client.pause_workflow(&workflow_id).await?;
workflow_client.resume_workflow(&workflow_id).await?;
workflow_client.terminate_workflow(&workflow_id, Some("no longer needed"), false).await?;
workflow_client.retry_workflow(&workflow_id, false).await?;
workflow_client.restart_workflow(&workflow_id, false).await?;
```

**Learn more:**
- [Workflow Management](docs/WORKFLOW.md) — Start, pause, resume, terminate, retry, search
- [Workflow Testing](docs/WORKFLOW_TESTING.md) — Unit testing with mock task outputs
- [Metadata Management](docs/METADATA.md) — Task & workflow definitions

## Troubleshooting

- **Worker stops polling**: `TaskHandler` monitors workers. Use `task_handler.is_healthy()` for health checks.
- **Connection issues**: Verify `CONDUCTOR_SERVER_URL` is correct and server is running.
- **Authentication failures**: For Orkes Conductor, ensure `CONDUCTOR_AUTH_KEY` and `CONDUCTOR_AUTH_SECRET` are valid.

---

## AI & LLM Workflows

Conductor supports AI-native workflows including agentic tool calling, RAG pipelines, and multi-agent orchestration.

**Agentic Workflows**

Build AI agents where LLMs dynamically select and call Rust workers as tools. See [examples/](examples/) for all examples.

| Example | Description |
|---------|-------------|
| [llm_chat_example.rs](examples/llm_chat_example.rs) | Automated multi-turn science Q&A between two LLMs |
| [llm_chat_human_in_loop.rs](examples/llm_chat_human_in_loop.rs) | Interactive chat with WAIT task pauses for user input |
| [multiagent_chat.rs](examples/multiagent_chat.rs) | Multi-agent discussion with expert, critic, and synthesizer |
| [function_calling_example.rs](examples/function_calling_example.rs) | LLM picks which function to call based on user queries |
| [agentic_workflow.rs](examples/agentic_workflow.rs) | AI agent with tool calling and switch-based routing |

**LLM and RAG Workflows**

| Example | Description |
|---------|-------------|
| [rag_workflow.rs](examples/rag_workflow.rs) | End-to-end RAG: text indexing, semantic search, answer generation |
| [vector_db_example.rs](examples/vector_db_example.rs) | Vector database operations with embedding generation |

```shell
# Automated multi-turn chat
cargo run --example llm_chat_example

# Multi-agent discussion
cargo run --example multiagent_chat

# RAG pipeline
cargo run --example rag_workflow
```

## Examples

See the examples directory for the full catalog. Key examples:

| Example | Description | Run |
|---------|-------------|-----|
| [worker_example.rs](examples/worker_example.rs) | End-to-end: sync + async workers, metrics | `cargo run --example worker_example` |
| [hello_world.rs](examples/hello_world.rs) | Minimal hello world | `cargo run --example hello_world` |
| [dynamic_workflow.rs](examples/dynamic_workflow.rs) | Build workflows programmatically | `cargo run --example dynamic_workflow` |
| [llm_chat_example.rs](examples/llm_chat_example.rs) | AI multi-turn chat | `cargo run --example llm_chat_example` |
| [rag_workflow.rs](examples/rag_workflow.rs) | RAG pipeline | `cargo run --example rag_workflow` |
| [task_context_example.rs](examples/task_context_example.rs) | Long-running tasks with TaskContext | `cargo run --example task_context_example` |
| [workflow_ops.rs](examples/workflow_ops.rs) | Pause, resume, terminate workflows | `cargo run --example workflow_ops` |
| [test_workflows.rs](examples/test_workflows.rs) | Unit testing workflows | `cargo run --example test_workflows` |
| [kitchensink.rs](examples/kitchensink.rs) | All task types (HTTP, JS, JQ, Switch) | `cargo run --example kitchensink` |

## API Journey Examples

End-to-end examples covering all APIs for each domain:

| Example | APIs | Run |
|---------|------|-----|
| [authorization_example.rs](examples/authorization_example.rs) | Authorization APIs | `cargo run --example authorization_example` |
| [metadata_journey.rs](examples/metadata_journey.rs) | Metadata APIs | `cargo run --example metadata_journey` |
| [schedule_journey.rs](examples/schedule_journey.rs) | Schedule APIs | `cargo run --example schedule_journey` |
| [prompt_journey.rs](examples/prompt_journey.rs) | Prompt APIs | `cargo run --example prompt_journey` |

## Documentation

| Document | Description |
|----------|-------------|
| [Worker Guide](docs/WORKER.md) | All worker patterns (function, closure, macro, async) |
| [Worker Configuration](WORKER_CONFIGURATION.md) | Hierarchical environment variable configuration |
| [Workflow Management](docs/WORKFLOW.md) | Start, pause, resume, terminate, retry, search |
| [Workflow Testing](docs/WORKFLOW_TESTING.md) | Unit testing with mock outputs |
| [Task Management](docs/TASK_MANAGEMENT.md) | Task operations |
| [Metadata](docs/METADATA.md) | Task & workflow definitions |
| [Authorization](docs/AUTHORIZATION.md) | Users, groups, applications, permissions |
| [Schedules](docs/SCHEDULE.md) | Workflow scheduling |
| [Secrets](docs/SECRET_MANAGEMENT.md) | Secret storage |
| [Prompts](docs/PROMPT.md) | AI/LLM prompt templates |
| [Integrations](docs/INTEGRATION.md) | AI/LLM provider integrations |
| [Metrics](METRICS.md) | Prometheus metrics collection |

## Support

- [Open an issue (SDK)](https://github.com/conductor-oss/conductor-rust/issues) for SDK bugs, questions, and feature requests
- [Open an issue (Conductor server)](https://github.com/conductor-oss/conductor/issues) for Conductor OSS server issues
- [Join the Conductor Slack](https://join.slack.com/t/orkes-conductor/shared_invite/zt-2vdbx239s-Eacdyqya9giNLHfrCavfaA) for community discussion and help
- [Orkes Community Forum](https://community.orkes.io/) for Q&A

## Frequently Asked Questions

**Is this the same as Netflix Conductor?**

Yes. Conductor OSS is the continuation of the original [Netflix Conductor](https://github.com/Netflix/conductor) repository after Netflix contributed the project to the open-source foundation.

**Is this project actively maintained?**

Yes. [Orkes](https://orkes.io) is the primary maintainer and offers an enterprise SaaS platform for Conductor across all major cloud providers.

**Can Conductor scale to handle my workload?**

Conductor was built at Netflix to handle massive scale and has been battle-tested in production environments processing millions of workflows. It scales horizontally to meet virtually any demand.

**Does Conductor support durable code execution?**

Yes. Conductor ensures workflows complete reliably even in the face of infrastructure failures, process crashes, or network issues.

**Are workflows always asynchronous?**

No. While Conductor excels at asynchronous orchestration, it also supports synchronous workflow execution when immediate results are required.

**Do I need to use a Conductor-specific framework?**

No. Conductor is language and framework agnostic. Use your preferred language and framework -- the [SDKs](https://github.com/conductor-oss/conductor#conductor-sdks) provide native integration for Python, Java, JavaScript, Go, C#, Rust, and more.

**Can I mix workers written in different languages?**

Yes. A single workflow can have workers written in Rust, Python, Java, Go, or any other supported language. Workers communicate through the Conductor server, not directly with each other.

**What Rust versions are supported?**

Rust 1.70 and above (2021 edition).

**Should I use `async fn` or regular `fn` for my workers?**

Use `async fn` for I/O-bound tasks (API calls, database queries) — the SDK uses async runtime for high concurrency with low overhead. Use regular functions for CPU-bound or blocking work. The SDK handles both patterns efficiently.

**How do I run workers in production?**

Workers are standard Rust applications. Deploy them as you would any Rust application -- in containers, VMs, or bare metal. Workers poll the Conductor server for tasks, so no inbound ports need to be opened.

**How do I test workflows without running a full Conductor server?**

The SDK provides a test framework that uses Conductor's `POST /api/workflow/test` endpoint to evaluate workflows with mock task outputs. See [Workflow Testing](docs/WORKFLOW_TESTING.md) for details.

## License

Apache 2.0
