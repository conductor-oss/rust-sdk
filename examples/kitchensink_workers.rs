use conductor::client::ConductorClient;
use conductor::worker::{FnWorker, TaskHandler, WorkerOutput};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Kitchensink Workers...");

    // Create client config
    let mut config = conductor::Configuration::default();

    // Allow overriding URL from env
    if let Ok(url) = env::var("CONDUCTOR_SERVER_URL") {
        config.server_api_url = url;
    }

    // Initialize client and task handler
    let _client = ConductorClient::new(config.clone())?;
    let mut handler = TaskHandler::new(config)?;

    // Worker logic helpers
    let process_task = |task_name: &str, task: &conductor::models::Task| {
        info!("Executing task: {} (ID: {})", task_name, task.task_id);

        let mut output: HashMap<String, Value> = HashMap::new();
        output.insert("source".to_string(), json!(task_name));
        output.insert("processed".to_string(), json!(true));

        // Pass through inputs to outputs for kitchensink flow
        for (k, v) in &task.input_data {
            output.insert(k.clone(), v.clone());
        }

        // Generate pseudo-random 0 or 1 for oddEven logic
        let odd_even = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            % 2;
        output.insert("oddEven".to_string(), json!(odd_even));

        // Add complex JSON output as requested
        let complex_data = json!({
            "statistics": {
                "executionTime": 0.45,
                "memoryUsageMb": 128,
                "cpuLoad": [0.1, 0.2, 0.15]
            },
            "metadata": {
                "tags": ["production", "v1", "kitchensink"],
                "prio": 1,
                "config": {
                    "retries": 3,
                    "timeout": "10s",
                    "features": {
                        "async": true,
                        "logging": "verbose"
                    }
                }
            },
            "results": [
                {
                    "id": 101,
                    "status": "success",
                    "values": [10, 20, 30]
                },
                {
                    "id": 102,
                    "status": "pending",
                    "values": []
                }
            ],
            "deeplyNested": {
                "level1": {
                    "level2": {
                        "level3": {
                            "message": "Hello from depth!"
                        }
                    }
                }
            }
        });
        output.insert("complexOutput".to_string(), complex_data);

        // Logic specific to kitchensink workflow
        if task_name == "task_1" {
            output.insert("mod".to_string(), json!(1));
            output.insert("oddEven".to_string(), json!(0));
        } else if task_name == "task_4" {
            // task_4 needs to output inputs and dynamicTasks for dynamic_fanout
            let mut dynamic_tasks = Vec::new();
            let mut dynamic_inputs = HashMap::new();

            for i in 0..3 {
                let sub_task_name = "task_1"; // Reusing task_1 for simplicity
                let sub_task_ref = format!("dyn_task_{}", i);

                let mut task_def = HashMap::new();
                task_def.insert("name", json!(sub_task_name));
                task_def.insert("taskReferenceName", json!(sub_task_ref));
                task_def.insert("type", json!("SIMPLE"));
                dynamic_tasks.push(task_def);

                let mut input = HashMap::new();
                input.insert("idx", json!(i));
                input.insert("mod", json!(i % 2));
                dynamic_inputs.insert(sub_task_ref, json!(input));
            }

            output.insert("dynamicTasks".to_string(), json!(dynamic_tasks));
            output.insert("inputs".to_string(), json!(dynamic_inputs));
        }

        info!("Completed task: {}", task_name);
        Ok(WorkerOutput::completed(output))
    };

    // Register Workers

    // task_1
    handler.add_worker(FnWorker::new("task_1", move |task| {
        let res = process_task("task_1", &task);
        async move { res }
    }));

    // task_4 (Generic)
    handler.add_worker(FnWorker::new("task_4", move |task| {
        let res = process_task("task_4", &task);
        async move { res }
    }));

    // task_10
    handler.add_worker(FnWorker::new("task_10", move |task| {
        let res = process_task("task_10", &task);
        async move { res }
    }));

    // task_11
    handler.add_worker(FnWorker::new("task_11", move |task| {
        let res = process_task("task_11", &task);
        async move { res }
    }));

    // task_30
    handler.add_worker(FnWorker::new("task_30", move |task| {
        let res = process_task("task_30", &task);
        async move { res }
    }));

    // task_2 (possible dynamic target)
    handler.add_worker(FnWorker::new("task_2", move |task| {
        let res = process_task("task_2", &task);
        async move { res }
    }));

    info!("Workers registered for: task_1, task_4, task_10, task_11, task_30, task_2");

    // Start polling
    handler.start().await?;

    info!("Workers running. Press Ctrl+C to exit.");

    // Run indefinitely
    tokio::signal::ctrl_c().await?;
    info!("Shutting down workers...");
    handler.stop().await?;

    Ok(())
}
