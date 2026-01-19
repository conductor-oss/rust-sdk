# Conductor OSS Rust SDK
[![CI Status](https://github.com/conductor-oss/conductor-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/conductor-oss/conductor-rust/actions/workflows/ci.yml)

Rust SDK for working with https://github.com/conductor-oss/conductor.

[Conductor](https://www.conductor-oss.org/) is the leading open-source orchestration platform allowing developers to build highly scalable distributed applications.

Check out the [official documentation for Conductor](https://orkes.io/content).

## ⭐ Conductor OSS

Show support for the Conductor OSS.  Please help spread the awareness by starring Conductor repo.

[![GitHub stars](https://img.shields.io/github/stars/conductor-oss/conductor.svg?style=social&label=Star&maxAge=)](https://GitHub.com/conductor-oss/conductor/)

## Content

<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->

- [Install Conductor Rust SDK](#install-conductor-rust-sdk)
  - [Get Conductor Rust SDK](#get-conductor-rust-sdk)
- [Hello World Application Using Conductor](#hello-world-application-using-conductor)
  - [Step 1: Create Workflow](#step-1-create-workflow)
    - [Creating Workflows by Code](#creating-workflows-by-code)
    - [(Alternatively) Creating Workflows in JSON](#alternatively-creating-workflows-in-json)
  - [Step 2: Write Task Worker](#step-2-write-task-worker)
  - [Step 3: Write _Hello World_ Application](#step-3-write-_hello-world_-application)
- [Running Workflows on Conductor Standalone (Installed Locally)](#running-workflows-on-conductor-standalone-installed-locally)
  - [Setup Environment Variable](#setup-environment-variable)
  - [Start Conductor Server](#start-conductor-server)
  - [Execute Hello World Application](#execute-hello-world-application)
- [Running Workflows on Orkes Conductor](#running-workflows-on-orkes-conductor)
- [Learn More about Conductor Rust SDK](#learn-more-about-conductor-rust-sdk)
- [Create and Run Conductor Workers](#create-and-run-conductor-workers)
- [Writing Workers](#writing-workers)
  - [Implementing Workers](#implementing-workers)
  - [Managing Workers in Application](#managing-workers-in-application)
  - [Design Principles for Workers](#design-principles-for-workers)
    - [System Task Workers](#system-task-workers)
    - [Wait Task](#wait-task)
      - [Using Code to Create Wait Task](#using-code-to-create-wait-task)
      - [JSON Configuration](#json-configuration)
    - [HTTP Task](#http-task)
      - [Using Code to Create HTTP Task](#using-code-to-create-http-task)
      - [JSON Configuration](#json-configuration-1)
    - [Javascript Executor Task](#javascript-executor-task)
      - [Using Code to Create Inline Task](#using-code-to-create-inline-task)
      - [JSON Configuration](#json-configuration-2)
    - [JSON Processing using JQ](#json-processing-using-jq)
      - [Using Code to Create JSON JQ Transform Task](#using-code-to-create-json-jq-transform-task)
      - [JSON Configuration](#json-configuration-3)
  - [Worker vs. Microservice/HTTP Endpoints](#worker-vs-microservicehttp-endpoints)
  - [Deploying Workers in Production](#deploying-workers-in-production)
- [Create Conductor Workflows](#create-conductor-workflows)
  - [Conductor Workflows](#conductor-workflows)
  - [Creating Workflows](#creating-workflows)
    - [Execute Dynamic Workflows Using Code](#execute-dynamic-workflows-using-code)
    - [Kitchen-Sink Workflow](#kitchen-sink-workflow)
  - [Executing Workflows](#executing-workflows)
    - [Execute Workflow Asynchronously](#execute-workflow-asynchronously)
    - [Execute Workflow Synchronously](#execute-workflow-synchronously)
  - [Managing Workflow Executions](#managing-workflow-executions)
  - [Get Execution Status](#get-execution-status)
  - [Update Workflow State Variables](#update-workflow-state-variables)
  - [Terminate Running Workflows](#terminate-running-workflows)
  - [Retry Failed Workflows](#retry-failed-workflows)
  - [Restart Workflows](#restart-workflows)
  - [Rerun Workflow from a Specific Task](#rerun-workflow-from-a-specific-task)
  - [Pause Running Workflow](#pause-running-workflow)
  - [Resume Paused Workflow](#resume-paused-workflow)
  - [Searching for Workflows](#searching-for-workflows)
  - [Handling Failures, Retries and Rate Limits](#handling-failures-retries-and-rate-limits)
    - [Retries](#retries)
    - [Rate Limits](#rate-limits)
    - [Task Registration](#task-registration)
    - [Update Task Definition:](#update-task-definition)
- [Using Conductor in Your Application](#using-conductor-in-your-application)
  - [Adding Conductor SDK to Your Application](#adding-conductor-sdk-to-your-application)
  - [Testing Workflows](#testing-workflows)
    - [Example Unit Testing Application](#example-unit-testing-application)
  - [Workflow Deployments Using CI/CD](#workflow-deployments-using-cicd)
  - [Versioning Workflows](#versioning-workflows)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

## Install Conductor Rust SDK

### Get Conductor Rust SDK

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

## Hello World Application Using Conductor

In this section, we will create a simple "Hello World" application that executes a "greetings" workflow managed by Conductor.

### Step 1: Create Workflow

#### Creating Workflows by Code

Create [greetings_workflow.rs](examples/helloworld/greetings_workflow.rs) with the following:

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

pub fn greetings_workflow() -> WorkflowDef {
    WorkflowDef::new("greetings")
        .with_description("Sample greetings workflow")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("greet", "greet_ref")
                .with_input_param("name", "${workflow.input.name}")
        )
        .with_output_param("result", "${greet_ref.output.result}")
}
```

#### (Alternatively) Creating Workflows in JSON

Create `greetings_workflow.json` with the following:

```json
{
  "name": "greetings",
  "description": "Sample greetings workflow",
  "version": 1,
  "tasks": [
    {
      "name": "greet",
      "taskReferenceName": "greet_ref",
      "type": "SIMPLE",
      "inputParameters": {
        "name": "${workflow.input.name}"
      }
    }
  ],
  "timeoutPolicy": "TIME_OUT_WF",
  "timeoutSeconds": 60
}
```

Workflows must be registered to the Conductor server. Use the API to register the greetings workflow from the JSON file above:
```shell
curl -X POST -H "Content-Type:application/json" \
http://localhost:8080/api/metadata/workflow -d @greetings_workflow.json
```
> [!note]
> To use the Conductor API, the Conductor server must be up and running (see [Running over Conductor standalone (installed locally)](#running-workflows-on-conductor-standalone-installed-locally)).

### Step 2: Write Task Worker

Using Rust, a worker represents a function with the `#[worker]` macro or using `FnWorker`. Create [greetings_worker.rs](examples/helloworld/greetings_worker.rs) file as illustrated below:

> [!note]
> A single workflow can have task workers written in different languages and deployed anywhere, making your workflow polyglot and distributed!

```rust
use conductor_macros::worker;

#[worker(name = "greet")]
async fn greet(name: String) -> String {
    format!("Hello {}", name)
}
```
Now, we are ready to write our main application, which will execute our workflow.

### Step 3: Write _Hello World_ Application

Let's add [helloworld.rs](examples/helloworld/helloworld.rs) with a `main` function:

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{StartWorkflowRequest, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};

mod greetings_worker;

fn greetings_workflow() -> WorkflowDef {
    WorkflowDef::new("greetings")
        .with_description("Sample greetings workflow")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("greet", "greet_ref")
                .with_input_param("name", "${workflow.input.name}")
        )
        .with_output_param("result", "${greet_ref.output.result}")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The app is connected to http://localhost:8080/api by default
    let api_config = Configuration::default();

    let client = ConductorClient::new(api_config.clone())?;

    // Registering the workflow (Required only when the app is executed the first time)
    let workflow = greetings_workflow();
    client.metadata_client()
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    // Starting the worker polling mechanism
    let mut task_handler = TaskHandler::new(api_config.clone())?;
    task_handler.add_worker(greetings_worker::greet_worker());
    task_handler.start().await?;

    let workflow_run = client.workflow_client()
        .execute_workflow(
            &StartWorkflowRequest::new("greetings")
                .with_version(1)
                .with_input_value("name", "Orkes"),
            std::time::Duration::from_secs(10),
        )
        .await?;

    println!("\nworkflow result: {:?}\n", workflow_run.output.get("result"));
    println!("see the workflow execution here: {}/execution/{}\n", 
        api_config.ui_host, workflow_run.workflow_id);
    
    task_handler.stop().await?;

    Ok(())
}
```
## Running Workflows on Conductor Standalone (Installed Locally)

### Setup Environment Variable

Set the following environment variable to point the SDK to the Conductor Server API endpoint:

```shell
export CONDUCTOR_SERVER_URL=http://localhost:8080/api
```
### Start Conductor Server

To start the Conductor server in a standalone mode from a Docker image, type the command below:

```shell
docker run --init -p 8080:8080 -p 5000:5000 conductoross/conductor-standalone:3.15.0
```
To ensure the server has started successfully, open Conductor UI on http://localhost:5000.

### Execute Hello World Application

To run the application, type the following command:

```shell
cargo run --example hello_world
```

Now, the workflow is executed, and its execution status can be viewed from Conductor UI (http://localhost:5000).

Navigate to the **Executions** tab to view the workflow execution.

## Running Workflows on Orkes Conductor

For running the workflow in Orkes Conductor,

- Update the Conductor server URL to your cluster name.

```shell
export CONDUCTOR_SERVER_URL=https://[cluster-name].orkesconductor.io/api
```

- If you want to run the workflow on the Orkes Conductor Playground, set the Conductor Server variable as follows:

```shell
export CONDUCTOR_SERVER_URL=https://developer.orkescloud.com/api
```

- Orkes Conductor requires authentication. [Obtain the key and secret from the Conductor server](https://orkes.io/content/how-to-videos/access-key-and-secret) and set the following environment variables.

```shell
export CONDUCTOR_AUTH_KEY=your_key
export CONDUCTOR_AUTH_SECRET=your_key_secret
```

Run the application and view the execution status from Conductor's UI Console.

> [!NOTE]
> That's it - you just created and executed your first distributed Rust app!

## Learn More about Conductor Rust SDK

There are three main ways you can use Conductor when building durable, resilient, distributed applications.

1. Write service workers that implement business logic to accomplish a specific goal - such as initiating payment transfer, getting user information from the database, etc.
2. Create Conductor workflows that implement application state - A typical workflow implements the saga pattern.
3. Use Conductor SDK and APIs to manage workflows from your application.

## Create and Run Conductor Workers

## Writing Workers

A Workflow task represents a unit of business logic that achieves a specific goal, such as checking inventory, initiating payment transfer, etc. A worker implements a task in the workflow.


### Implementing Workers

The workers can be implemented by writing a simple Rust function and annotating the function with the `#[worker]` macro, or by using `FnWorker`. Conductor workers are services (similar to microservices) that follow the [Single Responsibility Principle](https://en.wikipedia.org/wiki/Single_responsibility_principle).

Workers can be hosted along with the workflow or run in a distributed environment where a single workflow uses workers deployed and running in different machines/VMs/containers. Whether to keep all the workers in the same application or run them as a distributed application is a design and architectural choice. Conductor is well suited for both kinds of scenarios.

You can create or convert any existing Rust function to a distributed worker by adding `#[worker]` macro to it. Here is a simple worker that takes `name` as input and returns greetings:

```rust
use conductor_macros::worker;

#[worker(name = "greetings")]
async fn greetings(name: String) -> String {
    format!("Hello, {}", name)
}
```

**Using FnWorker (closure-based):**

```rust
use conductor::worker::{FnWorker, WorkerOutput};
use conductor::models::Task;

let greetings_worker = FnWorker::new("greetings", |task: Task| async move {
    let name = task.get_input_string("name").unwrap_or_default();
    Ok(WorkerOutput::completed_with_result(format!("Hello, {}", name)))
})
.with_thread_count(10)
.with_poll_interval_millis(100);
```

**With TaskContext for metadata access:**

```rust
use conductor::worker::TaskContext;
use conductor_macros::worker;

#[worker(name = "fetch_data", thread_count = 50)]
async fn fetch_data(ctx: TaskContext, url: String) -> serde_json::Value {
    // Access task metadata
    println!("Task ID: {}", ctx.task_id());
    println!("Poll count: {}", ctx.poll_count());
    
    if ctx.is_first_poll() {
        println!("First poll - initializing...");
    }
    
    // Fetch data...
    serde_json::json!({"url": url, "status": "fetched"})
}
```

A worker can take inputs which are primitives - `String`, `i32`, `f64`, `bool` etc. or can be complex data structures.

Here is an example worker that uses a struct as part of the worker input.

```rust
use conductor_macros::worker;
use serde::Deserialize;

#[derive(Deserialize)]
struct OrderInfo {
    order_id: i32,
    sku: String,
    quantity: i32,
    sku_price: f64,
}

#[worker(name = "process_order")]
async fn process_order(order_info: OrderInfo) -> String {
    format!("order: {}", order_info.order_id)
}
```

### Managing Workers in Application

Workers use a polling mechanism (with a long poll) to check for any available tasks from the server periodically. The startup and shutdown of workers are handled by the `TaskHandler` struct.

```rust
use conductor::{
    configuration::Configuration,
    worker::{FnWorker, TaskHandler, WorkerOutput},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // points to http://localhost:8080/api by default
    let api_config = Configuration::default();

    // Create workers
    let greetings_worker = FnWorker::new("greetings", |task| async move {
        let name = task.get_input_string("name").unwrap_or_default();
        Ok(WorkerOutput::completed_with_result(format!("Hello, {}", name)))
    });

    let mut task_handler = TaskHandler::new(api_config)?;
    task_handler.add_worker(greetings_worker);
    
    // start worker polling
    task_handler.start().await?;

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    // Call to stop the workers when the application is ready to shutdown
    task_handler.stop().await?;

    Ok(())
}
```

**Worker Configuration:** Workers support hierarchical configuration via environment variables, allowing you to override settings at deployment without code changes:

```bash
# Global configuration (applies to all workers) - Unix format recommended
export CONDUCTOR_WORKER_ALL_DOMAIN=production
export CONDUCTOR_WORKER_ALL_POLL_INTERVAL_MILLIS=250
export CONDUCTOR_WORKER_ALL_THREAD_COUNT=20

# Worker-specific configuration (overrides global)
export CONDUCTOR_WORKER_GREETINGS_THREAD_COUNT=50
export CONDUCTOR_WORKER_VALIDATE_ORDER_DOMAIN=premium

# Runtime control (pause/resume workers without code changes)
export CONDUCTOR_WORKER_ALL_PAUSED=true  # Maintenance mode
```

**Configuration Priority:** Worker-specific > Global > Code defaults

For detailed configuration options, see [WORKER_CONFIGURATION.md](WORKER_CONFIGURATION.md).

**Monitoring:** Enable Prometheus metrics with built-in HTTP server:

```rust
use conductor::metrics::MetricsSettings;

let mut task_handler = TaskHandler::new(api_config)?;
task_handler.enable_metrics(
    MetricsSettings::new()
        .with_http_port(9090)
);
// Metrics available at: http://localhost:9090/metrics
```

For more details, see [docs/WORKER.md](docs/WORKER.md).

### Design Principles for Workers

Each worker embodies the design pattern and follows certain basic principles:

1. Workers are stateless and do not implement a workflow-specific logic.
2. Each worker executes a particular task and produces well-defined output given specific inputs.
3. Workers are meant to be idempotent (Should handle cases where the partially executed task, due to timeouts, etc, gets rescheduled).
4. Workers do not implement the logic to handle retries, etc., that is taken care of by the Conductor server.

#### System Task Workers

A system task worker is a pre-built, general-purpose worker in your Conductor server distribution.

System tasks automate repeated tasks such as calling an HTTP endpoint, executing lightweight ECMA-compliant javascript code, publishing to an event broker, etc.

#### Wait Task

> [!tip]
> Wait is a powerful way to have your system wait for a specific trigger, such as an external event, a particular date/time, or duration, such as 2 hours, without having to manage threads, background processes, or jobs.

##### Using Code to Create Wait Task

```rust
use conductor::models::WorkflowTask;

// waits for 2 seconds before scheduling the next task
let wait_for_two_sec = WorkflowTask::wait("wait_for_2_sec")
    .with_wait_for_seconds(2);

// wait until end of jan
let wait_till_jan = WorkflowTask::wait("wait_till_jan")
    .with_wait_until("2024-01-31 00:00 UTC");

// waits until an API call or an event is triggered
let wait_for_signal = WorkflowTask::wait("wait_till_jan_end");
```
##### JSON Configuration

```json
{
  "name": "wait",
  "taskReferenceName": "wait_till_jan_end",
  "type": "WAIT",
  "inputParameters": {
    "until": "2024-01-31 00:00 UTC"
  }
}
```
#### HTTP Task

Make a request to an HTTP(S) endpoint. The task allows for GET, PUT, POST, DELETE, HEAD, and PATCH requests.

##### Using Code to Create HTTP Task

```rust
use conductor::models::WorkflowTask;

let http_task = WorkflowTask::http("call_remote_api", "https://orkes-api-tester.orkesconductor.com/api");
```

##### JSON Configuration

```json
{
  "name": "http_task",
  "taskReferenceName": "http_task_ref",
  "type" : "HTTP",
  "uri": "https://orkes-api-tester.orkesconductor.com/api",
  "method": "GET"
}
```

#### Javascript Executor Task

Execute ECMA-compliant Javascript code. It is useful when writing a script for data mapping, calculations, etc.

##### Using Code to Create Inline Task

```rust
use conductor::models::WorkflowTask;

let say_hello_js = r#"
function greetings() {
    return {
        "text": "hello " + $.name
    }
}
greetings();
"#;

let js = WorkflowTask::inline("hello_script", say_hello_js)
    .with_input_param("name", "${workflow.input.name}");
```
##### JSON Configuration

```json
{
  "name": "inline_task",
  "taskReferenceName": "inline_task_ref",
  "type": "INLINE",
  "inputParameters": {
    "expression": " function greetings() {\n  return {\n            \"text\": \"hello \" + $.name\n        }\n    }\n    greetings();",
    "evaluatorType": "graaljs",
    "name": "${workflow.input.name}"
  }
}
```

#### JSON Processing using JQ

[Jq](https://jqlang.github.io/jq/) is like sed for JSON data - you can slice, filter, map, and transform structured data with the same ease that sed, awk, grep, and friends let you play with text.

##### Using Code to Create JSON JQ Transform Task

```rust
use conductor::models::WorkflowTask;

let jq_script = "{ key3: (.key1.value1 + .key2.value2) }";

let jq = WorkflowTask::jq_transform("jq_process", jq_script);
```
##### JSON Configuration

```json
{
  "name": "json_transform_task",
  "taskReferenceName": "json_transform_task_ref",
  "type": "JSON_JQ_TRANSFORM",
  "inputParameters": {
    "key1": "k1",        
    "key2": "k2",
    "queryExpression": "{ key3: (.key1.value1 + .key2.value2) }"
  }
}
```

### Worker vs. Microservice/HTTP Endpoints

> [!tip] 
> Workers are a lightweight alternative to exposing an HTTP endpoint and orchestrating using HTTP tasks. Using workers is a recommended approach if you do not need to expose the service over HTTP or gRPC endpoints.

There are several advantages to this approach:

1. **No need for an API management layer** : Given there are no exposed endpoints and workers are self-load-balancing.
2. **Reduced infrastructure footprint** :  No need for an API gateway/load balancer.
3. All the communication is initiated by workers using polling - avoiding the need to open up any incoming TCP ports.
4. Workers **self-regulate** when busy; they only poll as much as they can handle. Backpressure handling is done out of the box.
5. Workers can be scaled up/down quickly based on the demand by increasing the number of instances.

### Deploying Workers in Production

Conductor workers can run in the cloud-native environment or on-prem and can easily be deployed like any other Rust application. Workers can run a containerized environment, VMs, or bare metal like you would deploy your other Rust applications.

## Create Conductor Workflows

### Conductor Workflows

Workflow can be defined as the collection of tasks and operators that specify the order and execution of the defined tasks. This orchestration occurs in a hybrid ecosystem that encircles serverless functions, microservices, and monolithic applications.

This section will dive deeper into creating and executing Conductor workflows using Rust SDK.


### Creating Workflows

Conductor lets you create the workflows using either Rust or JSON as the configuration.

Using Rust as code to define and execute workflows lets you build extremely powerful, dynamic workflows and run them on Conductor.

When the workflows are relatively static, they can be designed using the Orkes UI (available when using Orkes Conductor) and APIs or SDKs to register and run the workflows.

Both the code and configuration approaches are equally powerful and similar in nature to how you treat Infrastructure as Code.

#### Execute Dynamic Workflows Using Code

For cases where the workflows cannot be created statically ahead of time, Conductor is a powerful dynamic workflow execution platform that lets you create very complex workflows in code and execute them. It is useful when the workflow is unique for each execution.

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{StartWorkflowRequest, WorkflowDef, WorkflowTask},
    worker::{FnWorker, TaskHandler, WorkerOutput},
};

// FnWorker for get_user_email
fn get_user_email_worker() -> FnWorker {
    FnWorker::new("get_user_email", |task| async move {
        let userid = task.get_input_string("userid").unwrap_or_default();
        Ok(WorkerOutput::completed_with_result(format!("{}@example.com", userid)))
    })
}

// FnWorker for send_email
fn send_email_worker() -> FnWorker {
    FnWorker::new("send_email", |task| async move {
        let email = task.get_input_string("email").unwrap_or_default();
        let subject = task.get_input_string("subject").unwrap_or_default();
        let body = task.get_input_string("body").unwrap_or_default();
        println!("sending email to {} with subject {} and body {}", email, subject, body);
        Ok(WorkerOutput::complete())
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // defaults to reading the configuration using following env variables
    // CONDUCTOR_SERVER_URL : conductor server e.g. https://developer.orkescloud.com/api
    // CONDUCTOR_AUTH_KEY : API Authentication Key
    // CONDUCTOR_AUTH_SECRET: API Auth Secret
    let api_config = Configuration::default();

    let mut task_handler = TaskHandler::new(api_config.clone())?;
    task_handler.add_worker(get_user_email_worker());
    task_handler.add_worker(send_email_worker());
    // Start Polling
    task_handler.start().await?;

    let client = ConductorClient::new(api_config.clone())?;

    // Create dynamic workflow
    let workflow = WorkflowDef::new("dynamic_workflow")
        .with_version(1)
        .with_task(
            WorkflowTask::simple("get_user_email", "get_user_email_ref")
                .with_input_param("userid", "${workflow.input.userid}")
        )
        .with_task(
            WorkflowTask::simple("send_email", "send_email_ref")
                .with_input_param("email", "${get_user_email_ref.output.result}")
                .with_input_param("subject", "Hello from Orkes")
                .with_input_param("body", "Test Email")
        )
        .with_output_param("email", "${get_user_email_ref.output.result}");

    // Register and execute
    client.metadata_client()
        .register_or_update_workflow_def(&workflow, true)
        .await?;

    // Run the workflow
    let result = client.workflow_client()
        .execute_workflow(
            &StartWorkflowRequest::new("dynamic_workflow")
                .with_version(1)
                .with_input_value("userid", "user_a"),
            std::time::Duration::from_secs(10),
        )
        .await?;

    println!("\nworkflow output: {:?}\n", result.output);

    // Stop Polling
    task_handler.stop().await?;

    Ok(())
}
```

```shell
>> cargo run --example dynamic_workflow

sending email to user_a@example.com with subject Hello from Orkes and body Test Email

workflow output: {"email": "user_a@example.com"}
```
See [dynamic_workflow.rs](examples/dynamic_workflow.rs) for a fully functional example.

#### Kitchen-Sink Workflow 

For a more complex workflow example with all the supported features, see [kitchensink.rs](examples/kitchensink.rs).

### Executing Workflows

The [WorkflowClient](src/client/workflow_client.rs) provides all the APIs required to work with workflow executions.

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
};

let api_config = Configuration::default();
let client = ConductorClient::new(api_config)?;
let workflow_client = client.workflow_client();
```
#### Execute Workflow Asynchronously

Useful when workflows are long-running.

```rust
use conductor::models::StartWorkflowRequest;

let request = StartWorkflowRequest::new("hello")
    .with_version(1)
    .with_input_value("name", "Orkes");

// workflow id is the unique execution id associated with this execution
let workflow_id = workflow_client.start_workflow(&request).await?;
```
#### Execute Workflow Synchronously

Applicable when workflows complete very quickly - usually under 20-30 seconds.

```rust
use conductor::models::StartWorkflowRequest;
use std::time::Duration;

let request = StartWorkflowRequest::new("hello")
    .with_version(1)
    .with_input_value("name", "Orkes");

let workflow_run = workflow_client
    .execute_workflow(&request, Duration::from_secs(12))
    .await?;
```


### Managing Workflow Executions
> [!note] 
> See [workflow_ops.rs](examples/workflow_ops.rs) for a fully working application that demonstrates working with the workflow executions and sending signals to the workflow to manage its state.

Workflows represent the application state. With Conductor, you can query the workflow execution state anytime during its lifecycle. You can also send signals to the workflow that determines the outcome of the workflow state.

[WorkflowClient](src/client/workflow_client.rs) is the client interface used to manage workflow executions.

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
};

let api_config = Configuration::default();
let client = ConductorClient::new(api_config)?;
let workflow_client = client.workflow_client();
```

### Get Execution Status

The following method lets you query the status of the workflow execution given the id. When the `include_tasks` is set, the response also includes all the completed and in-progress tasks.

```rust
workflow_client.get_workflow(workflow_id: &str, include_tasks: bool) -> Result<Workflow>
```

### Update Workflow State Variables

Variables inside a workflow are the equivalent of global variables in a program.

```rust
workflow_client.update_variables(workflow_id: &str, variables: HashMap<String, Value>) -> Result<()>
```

### Terminate Running Workflows

Used to terminate a running workflow. Any pending tasks are canceled, and no further work is scheduled for this workflow upon termination. A failure workflow will be triggered but can be avoided if `trigger_failure_workflow` is set to False.

```rust
workflow_client.terminate_workflow(workflow_id: &str, reason: Option<&str>, trigger_failure_workflow: bool) -> Result<()>
```

### Retry Failed Workflows

If the workflow has failed due to one of the task failures after exhausting the retries for the task, the workflow can still be resumed by calling the retry.

```rust
workflow_client.retry_workflow(workflow_id: &str, resume_subworkflow_tasks: bool) -> Result<()>
```

When a sub-workflow inside a workflow has failed, there are two options:

1. Re-trigger the sub-workflow from the start (Default behavior).
2. Resume the sub-workflow from the failed task (set  `resume_subworkflow_tasks` to True).

### Restart Workflows

A workflow in the terminal state (COMPLETED, TERMINATED, FAILED) can be restarted from the beginning. Useful when retrying from the last failed task is insufficient, and the whole workflow must be started again.

```rust
workflow_client.restart_workflow(workflow_id: &str, use_latest_def: bool) -> Result<()>
```

### Rerun Workflow from a Specific Task

In the cases where a workflow needs to be restarted from a specific task rather than from the beginning, rerun provides that option. When issuing the rerun command to the workflow, you can specify the task ID from where the workflow should be restarted (as opposed to from the beginning), and optionally, the workflow's input can also be changed.

```rust
workflow_client.rerun_workflow(workflow_id: &str, task_id: &str, new_input: Option<HashMap<String, Value>>, new_task_input: Option<HashMap<String, Value>>) -> Result<Workflow>
```

> [!tip]
> Rerun is one of the most powerful features Conductor has, giving you unparalleled control over the workflow restart.
> 

### Pause Running Workflow

A running workflow can be put to a PAUSED status. A paused workflow lets the currently running tasks complete but does not schedule any new tasks until resumed.

```rust
workflow_client.pause_workflow(workflow_id: &str) -> Result<()>
```

### Resume Paused Workflow

Resume operation resumes the currently paused workflow, immediately evaluating its state and scheduling the next set of tasks.

```rust
workflow_client.resume_workflow(workflow_id: &str) -> Result<()>
```

### Searching for Workflows

Workflow executions are retained until removed from the Conductor. This gives complete visibility into all the executions an application has - regardless of the number of executions. Conductor has a powerful search API that allows you to search for workflow executions.

```rust
workflow_client.search(start: i32, size: i32, free_text: &str, query: Option<&str>) -> Result<SearchResult<WorkflowSummary>>
```

* **free_text**: Free text search to look for specific words in the workflow and task input/output.
* **query** SQL-like query to search against specific fields in the workflow.

Here are the supported fields for **query**:

| Field       | Description     |
|-------------|-----------------|
| status      |The status of the workflow. |
| correlationId |The ID to correlate the workflow execution to other executions.   |
| workflowType |The name of the workflow. |
 | version     |The version of the workflow. |
|startTime|The start time of the workflow is in milliseconds.|


### Handling Failures, Retries and Rate Limits

Conductor lets you embrace failures rather than worry about the complexities introduced in the system to handle failures.

All the aspects of handling failures, retries, rate limits, etc., are driven by the configuration that can be updated in real time without re-deploying your application.

#### Retries

Each task in the Conductor workflow can be configured to handle failures with retries, along with the retry policy (linear, fixed, exponential backoff) and maximum number of retry attempts allowed.

See [Error Handling](https://orkes.io/content/error-handling) for more details.

#### Rate Limits

What happens when a task is operating on a critical resource that can only handle a few requests at a time? Tasks can be configured to have a fixed concurrency (X request at a time) or a rate (Y tasks/time window).


#### Task Registration

```rust
use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{TaskDef, RetryLogic, TimeoutPolicy},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_config = Configuration::default();
    let client = ConductorClient::new(api_config)?;
    let metadata_client = client.metadata_client();

    let task_def = TaskDef::new("task_with_retries")
        .with_retry(3, RetryLogic::LinearBackoff, 1)
        // only allow 3 tasks at a time to be in the IN_PROGRESS status
        .with_concurrent_exec_limit(3)
        // timeout the task if not polled within 60 seconds of scheduling
        .with_poll_timeout(60)
        // timeout the task if the task does not COMPLETE in 2 minutes
        .with_timeout(120, TimeoutPolicy::TimeOutWorkflow)
        // for the long running tasks, timeout if the task does not get updated in COMPLETED or IN_PROGRESS status in
        // 60 seconds after the last update
        .with_response_timeout(60)
        // only allow 100 executions in a 10-second window! -- Note, this is complementary to concurrent_exec_limit
        .with_rate_limit(100, 10);

    metadata_client.register_task_defs(&[task_def]).await?;
    
    Ok(())
}
```


```json
{
  "name": "task_with_retries",
  
  "retryCount": 3,
  "retryLogic": "LINEAR_BACKOFF",
  "retryDelaySeconds": 1,
  "backoffScaleFactor": 1,
  
  "timeoutSeconds": 120,
  "responseTimeoutSeconds": 60,
  "pollTimeoutSeconds": 60,
  "timeoutPolicy": "TIME_OUT_WF",
  
  "concurrentExecLimit": 3,
  
  "rateLimitPerFrequency": 0,
  "rateLimitFrequencyInSeconds": 1
}
```

#### Update Task Definition:

```shell
POST /api/metadata/taskdef -d @task_def.json
```

See [metadata_journey.rs](examples/metadata_journey.rs) for a detailed working app.

## Using Conductor in Your Application

Conductor SDKs are lightweight and can easily be added to your existing or new Rust app. This section will dive deeper into integrating Conductor in your application.

### Adding Conductor SDK to Your Application

Add to your `Cargo.toml`:

```toml
[dependencies]
conductor = "0.1"
tokio = { version = "1", features = ["full"] }
```

### Testing Workflows

Conductor SDK for Rust provides a complete feature testing framework for your workflow-based applications. The framework works well with any testing framework you prefer without imposing any specific framework.

The Conductor server provides a test endpoint `POST /api/workflow/test` that allows you to post a workflow along with the test execution data to evaluate the workflow.

The goal of the test framework is as follows:

1. Ability to test the various branches of the workflow.
2. Confirm the workflow execution and tasks given a fixed set of inputs and outputs.
3. Validate that the workflow completes or fails given specific inputs.

Here are example assertions from the test:

```rust
use conductor::models::{WorkflowTestRequest, WorkflowStatus};

let test_request = WorkflowTestRequest {
    name: workflow.name.clone(),
    version: Some(workflow.version),
    workflow_def: Some(workflow.clone()),
    task_ref_to_mock_output: task_ref_to_mock_output,
    ..Default::default()
};

let run = workflow_client.test_workflow(&test_request).await?;

println!("completed the test run");
println!("status: {:?}", run.status);
assert_eq!(run.status, WorkflowStatus::Completed);
```

> [!note]
> Workflow workers are your regular Rust functions and can be tested with any available testing framework.

#### Example Unit Testing Application

See [test_workflows.rs](examples/test_workflows.rs) for a fully functional example of how to test a moderately complex workflow with branches.

### Workflow Deployments Using CI/CD

> [!tip]
> Treat your workflow definitions just like your code. Suppose you are defining the workflows using UI. In that case, we recommend checking the JSON configuration into the version control and using your development workflow for CI/CD to promote the workflow definitions across various environments such as Dev, Test, and Prod.

Here is a recommended approach when defining workflows using JSON:

* Treat your workflow metadata as code.
* Check in the workflow and task definitions along with the application code.
* Use `POST /api/metadata/*` endpoints or MetadataClient to register/update workflows as part of the deployment process.
* Version your workflows. If there is a significant change, change the version field of the workflow. See versioning workflows below for more details.


### Versioning Workflows

A powerful feature of Conductor is the ability to version workflows. You should increment the version of the workflow when there is a significant change to the definition. You can run multiple versions of the workflow at the same time. When starting a new workflow execution, use the `version` field to specify which version to use. When omitted, the latest (highest-numbered) version is used.

* Versioning allows safely testing changes by doing canary testing in production or A/B testing across multiple versions before rolling out.
* A version can also be deleted, effectively allowing for "rollback" if required.
