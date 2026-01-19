# Conductor Rust SDK Documentation

Complete API reference and guides for the Conductor Rust SDK.

## Quick Start

```rust
use conductor::{Configuration, ConductorClient, TaskHandler, FnWorker, WorkerOutput};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let config = Configuration::new("http://localhost:8080/api");
    
    // For Orkes Cloud with authentication:
    // let config = Configuration::new("https://play.orkes.io/api")
    //     .with_auth("KEY_ID", "KEY_SECRET");

    // Create a simple worker
    let worker = FnWorker::new("my_task", |task| async move {
        let name: String = task.get_input("name").unwrap_or_else(|| "World".to_string());
        Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", name)))
    })
    .with_thread_count(5)
    .with_poll_interval_millis(100);

    // Start the task handler
    let mut handler = TaskHandler::new(config)?;
    handler.add_worker(worker);
    handler.start().await?;

    // Keep running until interrupted
    tokio::signal::ctrl_c().await?;
    handler.stop().await?;

    Ok(())
}
```

## Documentation Sections

### Core Concepts

| Document | Description |
|----------|-------------|
| [Worker](./WORKER.md) | Implementing task workers to process Conductor tasks |
| [Workflow](./WORKFLOW.md) | Managing workflow executions |
| [Metadata](./METADATA.md) | Managing workflow and task definitions |

### Task Management

| Document | Description |
|----------|-------------|
| [Task Management](./TASK_MANAGEMENT.md) | Polling, updating, and managing tasks |

### Administration

| Document | Description |
|----------|-------------|
| [Authorization](./AUTHORIZATION.md) | RBAC and permission management (Orkes) |
| [Schedule](./SCHEDULE.md) | Scheduling workflow executions |
| [Secrets](./SECRET_MANAGEMENT.md) | Managing secrets (Orkes) |

### AI & Integration

| Document | Description |
|----------|-------------|
| [Prompts](./PROMPT.md) | AI prompt template management (Orkes) |
| [Integration](./INTEGRATION.md) | Integration provider management (Orkes) |

## Installation

Add the SDK to your `Cargo.toml`:

```toml
[dependencies]
conductor = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Configuration

### Environment Variables

The SDK supports configuration via environment variables or a `.env` file:

```bash
# Server URL (default: http://localhost:8080/api)
export CONDUCTOR_SERVER_URL=http://localhost:8080/api

# UI URL for workflow execution links (auto-derived from server URL if not set)
export CONDUCTOR_UI_SERVER_URL=http://localhost:5000

# Authentication (for Orkes Conductor)
export CONDUCTOR_AUTH_KEY=your_key_id
export CONDUCTOR_AUTH_SECRET=your_key_secret

# Token TTL in minutes (default: 45)
# The SDK automatically refreshes tokens before expiration
export CONDUCTOR_AUTH_TOKEN_TTL_MINS=45

# Request timeout in seconds (default: 30)
export CONDUCTOR_TIMEOUT_SECS=30

# Debug mode (default: false)
export CONDUCTOR_DEBUG=true

# Worker-specific configuration
export CONDUCTOR_WORKER_MY_TASK_POLL_INTERVAL=100
export CONDUCTOR_WORKER_MY_TASK_THREAD_COUNT=5
export CONDUCTOR_WORKER_MY_TASK_DOMAIN=my_domain
```

### Using .env Files

The SDK automatically loads `.env` files from the current directory:

```bash
# .env
CONDUCTOR_SERVER_URL=https://play.orkes.io/api
CONDUCTOR_AUTH_KEY=your_key_id
CONDUCTOR_AUTH_SECRET=your_key_secret
```

### Programmatic Configuration

```rust
use conductor::Configuration;
use std::time::Duration;

// Basic configuration (uses environment variables if available)
let config = Configuration::default();

// Explicit server URL
let config = Configuration::new("http://localhost:8080/api");

// With authentication
let config = Configuration::new("https://play.orkes.io/api")
    .with_auth("KEY_ID", "KEY_SECRET");

// With custom settings
let config = Configuration::new("http://localhost:8080/api")
    .with_auth("KEY_ID", "KEY_SECRET")
    .with_token_ttl(Duration::from_secs(30 * 60))  // 30 minute token TTL
    .with_timeout(Duration::from_secs(60))          // 60 second request timeout
    .with_debug(true);
```

### Authentication

The SDK supports both **Orkes Conductor** (with authentication) and **open-source Conductor** (without authentication):

#### Orkes Conductor

```rust
// Configure with auth key/secret
let config = Configuration::new("https://play.orkes.io/api")
    .with_auth("KEY_ID", "KEY_SECRET");

// The SDK automatically:
// - Fetches tokens from /token endpoint
// - Refreshes tokens before TTL expiration (default: 45 minutes)
// - Retries requests if token expires mid-request
// - Uses exponential backoff on auth failures
```

#### Open-Source Conductor

```rust
// No authentication needed
let config = Configuration::new("http://localhost:8080/api");

// If auth credentials are provided but server doesn't require auth,
// the SDK detects this (404 on /token) and disables auth automatically
```

## Client Overview

The SDK provides several specialized clients:

| Client | Purpose |
|--------|---------|
| `ConductorClient` | Unified client providing access to all other clients |
| `WorkflowClient` | Start, manage, and monitor workflows |
| `TaskClient` | Poll for tasks, update task results |
| `MetadataClient` | Manage workflow and task definitions |
| `SchedulerClient` | Create and manage workflow schedules |
| `SecretClient` | Manage secrets (Orkes) |
| `AuthorizationClient` | Manage users, groups, and permissions (Orkes) |
| `PromptClient` | Manage AI prompt templates (Orkes) |
| `IntegrationClient` | Manage integrations (Orkes) |

### Getting Clients

```rust
use conductor::ConductorClient;

let client = ConductorClient::new(config)?;

// Access specialized clients
let workflow_client = client.workflow_client();
let metadata_client = client.metadata_client();
let task_client = client.task_client();
```

## Error Handling

The SDK uses a unified error type:

```rust
use conductor::error::{ConductorError, Result};

async fn example() -> Result<()> {
    let client = ConductorClient::new(config)?;
    
    match client.workflow_client().get_workflow("workflow_id", true).await {
        Ok(workflow) => println!("Got workflow: {}", workflow.workflow_id),
        Err(ConductorError::Server { status: 404, .. }) => println!("Workflow not found"),
        Err(ConductorError::Api { message, .. }) => println!("API error: {}", message),
        Err(e) => println!("Other error: {}", e),
    }
    
    Ok(())
}
```

## Features

- **Async/Await**: Fully async API using Tokio
- **Type Safety**: Strong typing with Rust's type system
- **High Performance**: Zero-copy task handling with `FnWorkerArc`
- **Concurrent Workers**: Support for multiple concurrent task executions
- **Graceful Shutdown**: Proper handling of in-flight tasks during shutdown
- **Metrics**: Built-in Prometheus metrics support
- **Event System**: Subscribe to worker lifecycle events

## Examples

See the [examples](../examples/) directory for complete working examples:

- `simple_worker.rs` - Basic worker implementation
- `workflow_management.rs` - Starting and managing workflows
- `concurrent_workers.rs` - High-throughput worker setup
- `workflow_with_workers.rs` - Complete workflow execution example

## See Also

- [Conductor Documentation](https://orkes.io/content/docs)
- [Orkes Cloud](https://orkes.io)
- [GitHub Repository](https://github.com/conductor-oss/conductor-rust)
