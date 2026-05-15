// Copyright 2024 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

mod simulated_task_worker;
mod workflow_governor;
mod workflow_status_probe;

use std::process;
use std::sync::Arc;

use conductor::client::ConductorClient;
use conductor::configuration::Configuration;
use conductor::metrics::MetricsSettings;
use conductor::models::{TaskDef, WorkflowDef, WorkflowTask};
use conductor::worker::TaskHandler;

use simulated_task_worker::SimulatedTaskWorker;
use workflow_governor::WorkflowGovernor;
use workflow_status_probe::WorkflowStatusProbe;

const WORKFLOW_NAME: &str = "rust_simulated_tasks_workflow";

struct WorkerDef {
    task_name: &'static str,
    codename: &'static str,
    sleep_seconds: u64,
}

const SIMULATED_WORKERS: &[WorkerDef] = &[
    WorkerDef {
        task_name: "rust_worker_0",
        codename: "quickpulse",
        sleep_seconds: 1,
    },
    WorkerDef {
        task_name: "rust_worker_1",
        codename: "whisperlink",
        sleep_seconds: 2,
    },
    WorkerDef {
        task_name: "rust_worker_2",
        codename: "shadowfetch",
        sleep_seconds: 3,
    },
    WorkerDef {
        task_name: "rust_worker_3",
        codename: "ironforge",
        sleep_seconds: 4,
    },
    WorkerDef {
        task_name: "rust_worker_4",
        codename: "deepcrawl",
        sleep_seconds: 5,
    },
];

fn env_int_or_default(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

async fn register_metadata(client: &ConductorClient) {
    let metadata_client = client.metadata_client();

    let task_defs: Vec<TaskDef> = SIMULATED_WORKERS
        .iter()
        .map(|def| TaskDef {
            name: def.task_name.to_string(),
            description: Some(format!(
                "Rust SDK harness simulated task ({}, default delay {}s)",
                def.codename, def.sleep_seconds
            )),
            retry_count: 1,
            timeout_seconds: 300,
            response_timeout_seconds: 300,
            ..Default::default()
        })
        .collect();

    if let Err(e) = metadata_client.register_task_defs(&task_defs).await {
        eprintln!("Failed to register task definitions: {}", e);
        process::exit(1);
    }

    let mut workflow = WorkflowDef::new(WORKFLOW_NAME)
        .with_version(1)
        .with_description("Rust SDK harness simulated task workflow")
        .with_owner("rust-sdk-harness@conductor.io");

    for def in SIMULATED_WORKERS {
        workflow = workflow.with_task(WorkflowTask::simple(def.task_name, def.codename));
    }

    if let Err(e) = metadata_client
        .register_or_update_workflow_def(&workflow, true)
        .await
    {
        eprintln!("Failed to register workflow: {}", e);
        process::exit(1);
    }

    println!(
        "Registered workflow {} with {} tasks",
        WORKFLOW_NAME,
        SIMULATED_WORKERS.len()
    );
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Configuration::from_env();
    let client = match ConductorClient::new(config.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create Conductor client: {}", e);
            process::exit(1);
        }
    };

    register_metadata(&client).await;

    let workflows_per_sec = env_int_or_default("HARNESS_WORKFLOWS_PER_SEC", 2);
    let batch_size = env_int_or_default("HARNESS_BATCH_SIZE", 20);
    let poll_interval_ms = env_int_or_default("HARNESS_POLL_INTERVAL_MS", 100);
    let probe_rate = env_int_or_default("HARNESS_PROBE_RATE_PER_SEC", 0);

    let mut handler = match TaskHandler::new(config) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to create task handler: {}", e);
            process::exit(1);
        }
    };

    let metrics_port: u16 = std::env::var("HARNESS_METRICS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9991);

    handler.enable_metrics(
        MetricsSettings::new()
            .with_http_port(metrics_port)
            .with_metrics_path("/metrics"),
    );
    println!("Prometheus metrics server started on port {}", metrics_port);

    for def in SIMULATED_WORKERS {
        let worker = SimulatedTaskWorker::new(
            def.task_name,
            def.codename,
            def.sleep_seconds,
            batch_size,
            poll_interval_ms as u64,
        );
        handler.add_worker(worker);
    }

    if let Err(e) = handler.start().await {
        eprintln!("Failed to start workers: {}", e);
        process::exit(1);
    }

    let workflow_client = handler.conductor_client().workflow_client();

    // Build governor, optionally wired to the status probe
    let probe_handle = if probe_rate > 0 {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(512);

        let probe = WorkflowStatusProbe::new(workflow_client.clone(), rx, probe_rate);
        let handle = tokio::spawn(async move { probe.run().await });

        let governor = Arc::new(
            WorkflowGovernor::new(workflow_client, WORKFLOW_NAME.to_string(), workflows_per_sec)
                .with_id_sink(tx),
        );
        let governor_handle = tokio::spawn({
            let governor = Arc::clone(&governor);
            async move { governor.run().await }
        });

        println!(
            "WorkflowStatusProbe enabled at {}/sec",
            probe_rate,
        );

        Some((governor_handle, handle))
    } else {
        let governor = Arc::new(WorkflowGovernor::new(
            workflow_client,
            WORKFLOW_NAME.to_string(),
            workflows_per_sec,
        ));
        let governor_handle = tokio::spawn({
            let governor = Arc::clone(&governor);
            async move { governor.run().await }
        });

        Some((governor_handle, tokio::spawn(async {})))
    };

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");

    println!("Shutting down...");
    if let Some((gov, probe)) = probe_handle {
        gov.abort();
        probe.abort();
    }

    if let Err(e) = handler.stop().await {
        eprintln!("Error stopping workers: {}", e);
    }
}
