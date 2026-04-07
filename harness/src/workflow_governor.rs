// Copyright 2024 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::client::WorkflowClient;
use conductor::models::StartWorkflowRequest;
use tokio::time::{self, Duration};

pub struct WorkflowGovernor {
    workflow_client: WorkflowClient,
    workflow_name: String,
    workflows_per_second: usize,
}

impl WorkflowGovernor {
    pub fn new(
        workflow_client: WorkflowClient,
        workflow_name: String,
        workflows_per_second: usize,
    ) -> Self {
        Self {
            workflow_client,
            workflow_name,
            workflows_per_second,
        }
    }

    pub async fn run(&self) {
        println!(
            "WorkflowGovernor started: workflow={}, rate={}/sec",
            self.workflow_name, self.workflows_per_second
        );

        let mut interval = time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;
            self.start_batch().await;
        }
    }

    async fn start_batch(&self) {
        for _ in 0..self.workflows_per_second {
            let request = StartWorkflowRequest::new(&self.workflow_name).with_version(1);
            if let Err(e) = self.workflow_client.start_workflow(&request).await {
                println!("Governor: error starting workflows: {}", e);
                return;
            }
        }
        println!(
            "Governor: started {} workflow(s)",
            self.workflows_per_second
        );
    }
}
