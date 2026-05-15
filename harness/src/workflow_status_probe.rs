// Copyright 2024 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::collections::VecDeque;
use conductor::client::WorkflowClient;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

const MAX_IDS: usize = 256;

/// Exercises UUID-bearing workflow endpoints to generate high-cardinality
/// traffic, validating that the path template system keeps the `uri` metric
/// label bounded. Off by default; enabled via `HARNESS_PROBE_RATE_PER_SEC`.
pub struct WorkflowStatusProbe {
    workflow_client: WorkflowClient,
    rx: mpsc::Receiver<String>,
    rate_per_sec: usize,
}

impl WorkflowStatusProbe {
    pub fn new(
        workflow_client: WorkflowClient,
        rx: mpsc::Receiver<String>,
        rate_per_sec: usize,
    ) -> Self {
        Self {
            workflow_client,
            rx,
            rate_per_sec,
        }
    }

    pub async fn run(mut self) {
        println!(
            "WorkflowStatusProbe started: rate={}/sec",
            self.rate_per_sec,
        );

        let mut ids: VecDeque<String> = VecDeque::with_capacity(MAX_IDS);
        let mut tick_count: u64 = 0;
        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            // Drain any new IDs from the governor
            while let Ok(id) = self.rx.try_recv() {
                if ids.len() >= MAX_IDS {
                    ids.pop_front();
                }
                ids.push_back(id);
            }

            if ids.is_empty() {
                continue;
            }

            for i in 0..self.rate_per_sec {
                let idx = ((tick_count as usize) * self.rate_per_sec + i) % ids.len();
                let id = &ids[idx];

                // Alternate between get_workflow and get_workflow_status
                if (tick_count as usize + i) % 2 == 0 {
                    match self.workflow_client.get_workflow(id, false).await {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Probe: get_workflow error: {}", e);
                        }
                    }
                } else {
                    match self
                        .workflow_client
                        .get_workflow_status(id, false, false)
                        .await
                    {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Probe: get_workflow_status error: {}", e);
                        }
                    }
                }
            }

            tick_count += 1;
        }
    }
}
