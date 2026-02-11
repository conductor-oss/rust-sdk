//! Task Configuration Example
//!
//! Demonstrates how to programmatically create and configure task definitions
//! with various settings like retries, timeouts, rate limits, and concurrency.
//!
//! ## Key Configuration Options
//! - `retry_count`: Number of retry attempts on failure
//! - `retry_logic`: LINEAR_BACKOFF, EXPONENTIAL_BACKOFF, FIXED
//! - `retry_delay_seconds`: Wait time between retries
//! - `concurrent_exec_limit`: Max concurrent executions
//! - `poll_timeout_seconds`: Task fails if not polled within this time
//! - `timeout_seconds`: Total execution timeout
//! - `response_timeout_seconds`: Timeout if no status update received
//! - `rate_limit_per_frequency`: Rate limit per time window
//! - `rate_limit_frequency_in_seconds`: Time window for rate limit
//!
//! ## Use Cases
//! - Programmatically managing task definitions (Infrastructure as Code)
//! - Setting task-level retry policies
//! - Configuring timeout and concurrency controls
//! - Implementing rate limiting for external API calls
//! - Creating task definitions as part of deployment automation
//!
//! ## Environment Variables
//! ```bash
//! export CONDUCTOR_SERVER_URL="https://your-conductor-server/api"
//! export CONDUCTOR_AUTH_KEY="your-key"
//! export CONDUCTOR_AUTH_SECRET="your-secret"
//! ```
//!
//! ## Run
//! ```bash
//! cargo run --example task_configure
//! ```

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{RetryLogic, TaskDef, TimeoutPolicy},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Task Configuration Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();

    // Track created tasks for cleanup
    let mut created_tasks = Vec::new();

    // ==========================================================================
    // Example 1: Basic Task with Retries
    // ==========================================================================
    println!("\n1. BASIC TASK WITH RETRIES");
    println!("{}", "-".repeat(40));

    let basic_task = TaskDef::new("rust_task_basic")
        .with_description("Basic task with retry configuration")
        .with_retry(3, RetryLogic::Fixed, 10); // 3 retries, fixed delay of 10 seconds

    println!("  Name: {}", basic_task.name);
    println!("  Retry Count: {:?}", basic_task.retry_count);
    println!("  Retry Logic: {:?}", basic_task.retry_logic);
    println!("  Retry Delay: {:?}s", basic_task.retry_delay_seconds);

    metadata_client.register_task_def(&basic_task).await?;
    created_tasks.push(basic_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 2: Task with Linear Backoff Retries
    // ==========================================================================
    println!("\n2. TASK WITH LINEAR BACKOFF");
    println!("{}", "-".repeat(40));

    let linear_backoff_task = TaskDef::new("rust_task_linear_backoff")
        .with_description("Task with linear backoff retry strategy")
        .with_retry(5, RetryLogic::LinearBackoff, 5)
        .with_timeout(120, TimeoutPolicy::Retry); // 2 minute timeout

    println!("  Name: {}", linear_backoff_task.name);
    println!("  Retry Count: {:?}", linear_backoff_task.retry_count);
    println!("  Retry Logic: {:?}", linear_backoff_task.retry_logic);
    println!(
        "  Retry Delay (base): {:?}s",
        linear_backoff_task.retry_delay_seconds
    );
    println!("  Timeout: {:?}s", linear_backoff_task.timeout_seconds);
    println!();
    println!("  Linear Backoff Pattern:");
    println!("    Attempt 1: Wait 5s");
    println!("    Attempt 2: Wait 10s (5 * 2)");
    println!("    Attempt 3: Wait 15s (5 * 3)");
    println!("    Attempt 4: Wait 20s (5 * 4)");
    println!("    Attempt 5: Wait 25s (5 * 5)");

    metadata_client.register_task_def(&linear_backoff_task).await?;
    created_tasks.push(linear_backoff_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 3: Task with Exponential Backoff
    // ==========================================================================
    println!("\n3. TASK WITH EXPONENTIAL BACKOFF");
    println!("{}", "-".repeat(40));

    let exp_backoff_task = TaskDef::new("rust_task_exponential_backoff")
        .with_description("Task with exponential backoff for external API calls")
        .with_retry(4, RetryLogic::ExponentialBackoff, 2)
        .with_timeout(300, TimeoutPolicy::Retry); // 5 minute timeout

    println!("  Name: {}", exp_backoff_task.name);
    println!("  Retry Count: {:?}", exp_backoff_task.retry_count);
    println!("  Retry Logic: {:?}", exp_backoff_task.retry_logic);
    println!(
        "  Retry Delay (base): {:?}s",
        exp_backoff_task.retry_delay_seconds
    );
    println!();
    println!("  Exponential Backoff Pattern:");
    println!("    Attempt 1: Wait 2s");
    println!("    Attempt 2: Wait 4s (2^2)");
    println!("    Attempt 3: Wait 8s (2^3)");
    println!("    Attempt 4: Wait 16s (2^4)");

    metadata_client.register_task_def(&exp_backoff_task).await?;
    created_tasks.push(exp_backoff_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 4: Task with Concurrency Limit
    // ==========================================================================
    println!("\n4. TASK WITH CONCURRENCY LIMIT");
    println!("{}", "-".repeat(40));

    let concurrent_task = TaskDef::new("rust_task_concurrent_limited")
        .with_description("Task with limited concurrent executions")
        .with_retry(3, RetryLogic::Fixed, 10)
        .with_timeout(120, TimeoutPolicy::Retry)
        .with_concurrent_limit(5); // Only 5 tasks can be IN_PROGRESS at a time

    println!("  Name: {}", concurrent_task.name);
    println!(
        "  Concurrent Exec Limit: {:?}",
        concurrent_task.concurrent_exec_limit
    );
    println!();
    println!("  Use Cases:");
    println!("    - Limit load on external APIs");
    println!("    - Control database connection usage");
    println!("    - Manage resource-intensive operations");

    metadata_client.register_task_def(&concurrent_task).await?;
    created_tasks.push(concurrent_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 5: Task with Rate Limiting
    // ==========================================================================
    println!("\n5. TASK WITH RATE LIMITING");
    println!("{}", "-".repeat(40));

    let rate_limited_task = TaskDef::new("rust_task_rate_limited")
        .with_description("Task with rate limiting for external API calls")
        .with_retry(3, RetryLogic::Fixed, 10)
        .with_timeout(120, TimeoutPolicy::Retry)
        .with_rate_limit(100, 10); // 100 executions per 10-second window

    println!("  Name: {}", rate_limited_task.name);
    println!(
        "  Rate Limit: {:?} per {:?}s",
        rate_limited_task.rate_limit_per_frequency, rate_limited_task.rate_limit_frequency_in_seconds
    );
    println!();
    println!("  Effective Rate: 10 executions/second max");
    println!();
    println!("  Use Cases:");
    println!("    - Respect API rate limits (e.g., 100 req/10s)");
    println!("    - Prevent overwhelming downstream services");
    println!("    - Cost control for metered APIs");

    metadata_client.register_task_def(&rate_limited_task).await?;
    created_tasks.push(rate_limited_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 6: Task with Response Timeout
    // ==========================================================================
    println!("\n6. TASK WITH RESPONSE TIMEOUT");
    println!("{}", "-".repeat(40));

    let response_timeout_task = TaskDef::new("rust_task_response_timeout")
        .with_description("Long-running task with response timeout")
        .with_retry(2, RetryLogic::Fixed, 30)
        .with_timeout(3600, TimeoutPolicy::Retry) // Total timeout: 1 hour
        .with_response_timeout(300); // Timeout if no status update in 5 minutes

    println!("  Name: {}", response_timeout_task.name);
    println!(
        "  Timeout (total): {:?}s (1 hour)",
        response_timeout_task.timeout_seconds
    );
    println!(
        "  Response Timeout: {:?}s (5 minutes)",
        response_timeout_task.response_timeout_seconds
    );
    println!();
    println!("  Behavior:");
    println!("    - Task can run for up to 1 hour total");
    println!("    - Task must send status updates every 5 minutes");
    println!("    - If no update received in 5 minutes, task fails");
    println!();
    println!("  Use Cases:");
    println!("    - Long-running batch jobs with heartbeat");
    println!("    - Data processing pipelines");
    println!("    - ML model training tasks");

    metadata_client.register_task_def(&response_timeout_task).await?;
    created_tasks.push(response_timeout_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 7: Task with Poll Timeout
    // ==========================================================================
    println!("\n7. TASK WITH POLL TIMEOUT");
    println!("{}", "-".repeat(40));

    let mut poll_timeout_task = TaskDef::new("rust_task_poll_timeout")
        .with_description("Task that must be picked up quickly")
        .with_retry(3, RetryLogic::Fixed, 10);
    
    // Set poll timeout directly (no builder method available)
    poll_timeout_task.poll_timeout_seconds = 60; // Fail if not polled within 60 seconds

    println!("  Name: {}", poll_timeout_task.name);
    println!(
        "  Poll Timeout: {:?}s",
        poll_timeout_task.poll_timeout_seconds
    );
    println!();
    println!("  Behavior:");
    println!("    - Task must be picked up by worker within 60 seconds");
    println!("    - If no worker polls, task is marked as TIMED_OUT");
    println!();
    println!("  Use Cases:");
    println!("    - Time-sensitive operations");
    println!("    - Detecting worker pool issues");
    println!("    - SLA enforcement");

    metadata_client.register_task_def(&poll_timeout_task).await?;
    created_tasks.push(poll_timeout_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Example 8: Complete Production Task
    // ==========================================================================
    println!("\n8. COMPLETE PRODUCTION TASK");
    println!("{}", "-".repeat(40));

    let mut production_task = TaskDef::new("rust_task_production")
        .with_description("Production-ready task with comprehensive configuration")
        .with_retry(3, RetryLogic::ExponentialBackoff, 5)
        .with_timeout(300, TimeoutPolicy::Retry)
        .with_response_timeout(60)
        .with_concurrent_limit(10)
        .with_rate_limit(1000, 60)
        .with_input_keys(vec!["orderId".to_string(), "customerId".to_string()])
        .with_output_keys(vec!["result".to_string(), "processedAt".to_string()]);
    
    // Set poll timeout directly
    production_task.poll_timeout_seconds = 30;

    println!("  Name: {}", production_task.name);
    println!("  Description: {:?}", production_task.description);
    println!();
    println!("  Retry Configuration:");
    println!("    - Retry Count: {:?}", production_task.retry_count);
    println!("    - Retry Logic: {:?}", production_task.retry_logic);
    println!("    - Retry Delay: {:?}s", production_task.retry_delay_seconds);
    println!();
    println!("  Timeout Configuration:");
    println!("    - Total Timeout: {:?}s", production_task.timeout_seconds);
    println!(
        "    - Response Timeout: {:?}s",
        production_task.response_timeout_seconds
    );
    println!(
        "    - Poll Timeout: {:?}s",
        production_task.poll_timeout_seconds
    );
    println!();
    println!("  Capacity Configuration:");
    println!(
        "    - Concurrent Limit: {:?}",
        production_task.concurrent_exec_limit
    );
    println!(
        "    - Rate Limit: {:?}/{:?}s",
        production_task.rate_limit_per_frequency, production_task.rate_limit_frequency_in_seconds
    );
    println!();
    println!("  Schema:");
    println!("    - Input Keys: {:?}", production_task.input_keys);
    println!("    - Output Keys: {:?}", production_task.output_keys);

    metadata_client.register_task_def(&production_task).await?;
    created_tasks.push(production_task.name.clone());
    println!("  [Registered]");

    // ==========================================================================
    // Summary
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("SUMMARY");
    println!("{}", "=".repeat(80));
    println!();
    println!("Created {} task definitions:", created_tasks.len());
    for task in &created_tasks {
        println!("  - {}", task);
    }

    // ==========================================================================
    // Cleanup
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CLEANUP");
    println!("{}", "=".repeat(80));
    println!();

    for task_name in &created_tasks {
        match metadata_client.delete_task_def(task_name).await {
            Ok(_) => println!("  Deleted: {}", task_name),
            Err(e) => println!("  Could not delete {}: {}", task_name, e),
        }
    }

    println!();
    println!("Task configuration example completed!");
    println!();
    println!("Best Practices:");
    println!("  1. Use exponential backoff for external API calls");
    println!("  2. Set appropriate concurrent limits to prevent overload");
    println!("  3. Configure rate limits to respect API quotas");
    println!("  4. Use response timeouts for long-running tasks");
    println!("  5. Define input/output keys for documentation");

    Ok(())
}
