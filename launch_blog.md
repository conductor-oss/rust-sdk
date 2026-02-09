# Rust Beyond the Metal: Durable, Async, and Agentic Workflows with Conductor

**By the Orkes Engineering Team**

Rust gives you memory safety and raw performance. Tokio gives you incredible concurrency. But what happens when your server vanishes? Or when an API call hangs for 3 days?

Handling state, retries, and long-running processes in distributed systems is notoriously hard. You often end up writing boilerplate for database checkpoints, retry queues, and timeout handlers—distracting you from the actual business logic.

Today, we’re excited to introduce the **[Orkes Conductor Rust SDK](https://github.com/conductor-oss/conductor-rust)**, a library that combines the safety and speed of Rust with the *durable execution* model of Netflix Conductor.

## For the Tokio Native: Built on the Shoulders of Giants

If you live in the Rust ecosystem, you know that `async` is the way. We didn't want to build a wrapper that fights the language. We built a native citizen.

The Rust SDK is powered by **Tokio** under the hood. From the HTTP layer (utilizing `reqwest`) to our worker polling mechanism, everything is designed to be non-blocking and efficient.

### How it works under the hood
When you start a worker in our SDK, we don't block an OS thread. Instead, we use `tokio::spawn` to manage lightweight tasks and `tokio::sync::Semaphore` to handle concurrency control.

```rust
// Internally, your workers run like this:
tokio::spawn(async move {
    // Acquire a permit from the semaphore (non-blocking wait)
    let _permit = semaphore.acquire().await.unwrap();
    
    // Execute your worker logic
    let result = worker.execute(&task).await;
    
    // ... handle result and update Conductor
});
```

This means a single container running our Rust SDK can easily handle thousands of concurrent tasks with minimal resource overhead, playing nicely with the rest of your `tower` middleware and `tracing` stack.

## Durable Execution: Code That Cannot Forget

"Durable execution" is like having a debugger that works across server restarts and network partitions.

In a standard Rust app, if your process crashes while waiting for an API response, that state is lost. You might have to check a database to see if "Step A" finished.

With Conductor, your workflow state is externalized.
1.  **You write a Worker**: A simple Rust function that does *one thing*.
2.  **Conductor Orchestrates**: It triggers your worker.
3.  **Persistence**: If your worker crashes, Conductor knows. It can retry on a different machine. If your worker succeeds, the input/output is persisted forever.

You get enhanced observability out of the box. You can visually trace the path of execution, inspect inputs/outputs at every step, and debug failures without digging through scattered logs.

## The New Frontier: Building High-Performance Agents

Orchestrating AI isn't just about calling an LLM API. It's about loops, memory, tool use, and—crucially—**reliability**.

The Conductor Rust SDK is positioned as the "nervous system" for your AI Agents.

*   **Model Agnostic**: Through Conductor's AI tasks, you can swap OpenAI for Anthropic, Gemini, or local models transparently. Your Rust code doesn't change.
*   **Memory & RAG**: Native tasks for generating embeddings and interacting with Vector DBs (Weaviate, Pinecone, etc.) give your agents long-term memory.

### Agentic Patterns with MCP (Model Context Protocol)

We support the **[Model Context Protocol](https://modelcontextprotocol.io/)**, allowing you to expose your high-performance Rust functions as "Tools" that LLMs can invoke autonomously.

Imagine a **"Paperclip Maximizer" Controller**:
1.  **Plan**: An `LLM_CHAT_COMPLETE` task asks an LLM to generate a manufacturing plan.
2.  **Safety Check**: A **Rust Worker** acts as a strict validator. It receives the plan, parses it, and enforces safety invariants (e.g., "resource_usage < limit"). This is compiled, type-safe Rust code checking the probabilistic output of an LLM.
3.  **Execute**: Only if the Rust validator approves does the workflow proceed to the execution phase.

This "Human-in-the-loop" (or "Rust-in-the-loop") pattern is essential for moving agents from prototypes to production.

## Show Me The Code

Defining a worker is as simple as annotating an async function:

```rust
use conductor_macros::worker_task;

#[worker_task(name = "process_payment")]
async fn process_payment(amount: u64) -> Result<PaymentResult> {
    if amount > 10000 {
        // This rejection is durable; Conductor records it.
        return Ok(PaymentResult::RequiresApproval);
    }
    
    // Call your payment gateway...
    Ok(PaymentResult::Success)
}
```

Launching the workflow from your client:

```rust
let workflow = WorkflowBuilder::new("order_flow")
    .add(SimpleTask::new("process_payment", amount))
    .build();

let id = client.start_workflow(&workflow).await?;
println!("Started durable workflow: {}", id);
```

## Performance & Efficiency

One of the biggest reasons to choose Rust for orchestration is efficiency. 

We've seen organizations run thousands of concurrent workflows on a fraction of the hardware required for JVM or Python equivalents. By leveraging Rust's type system and zero-cost abstractions, you ensure that your orchestration layer is never the bottleneck in your distributed system.

## Ready to Build?

The Orkes Conductor Rust SDK is open source and ready for you to build unbreakable systems.

*   **[Get the Crate](https://crates.io/crates/conductor)**
*   **[Explore the Repository](https://github.com/conductor-oss/conductor-rust)**
*   **[Join our Slack Community](https://orkes.io/slack)**

Whether you are migrating a fragile cron job or building the next generation of autonomous agents, we can't wait to see what you build with Rust and Conductor.
