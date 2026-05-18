// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::client::{ConductorClient, MetadataClient, SchemaClient, TaskClient};
use crate::configuration::{resolve_worker_config, Configuration, WorkerConfig};
use crate::error::{ConductorError, Result};
use crate::events::{EventDispatcher, TaskRunnerEventsListener};
use crate::http::ApiClient;
use crate::metrics::{MetricsCollector, MetricsSettings};
use crate::models::{SchemaDef, TaskDef};

use super::{TaskRunner, Worker};

/// Task handler for managing multiple workers
pub struct TaskHandler {
    #[allow(dead_code)] // Stored for potential future use (e.g., reconfiguration)
    config: Configuration,
    api_client: ApiClient,
    event_dispatcher: EventDispatcher,
    workers: Vec<Arc<dyn Worker>>,
    runners: Vec<Arc<TaskRunner>>,
    handles: Vec<JoinHandle<()>>,
    metrics_collector: Option<Arc<MetricsCollector>>,
    metrics_handle: Option<JoinHandle<()>>,
}

impl TaskHandler {
    /// Create a new task handler with the given configuration
    pub fn new(config: Configuration) -> Result<Self> {
        let api_client = ApiClient::new(config.clone())?;
        let event_dispatcher = EventDispatcher::new();

        Ok(Self {
            config,
            api_client,
            event_dispatcher,
            workers: Vec::new(),
            runners: Vec::new(),
            handles: Vec::new(),
            metrics_collector: None,
            metrics_handle: None,
        })
    }

    /// Create a builder for more flexible configuration
    pub fn builder(config: Configuration) -> TaskHandlerBuilder {
        TaskHandlerBuilder::new(config)
    }

    /// Add a worker to the handler
    pub fn add_worker(&mut self, worker: impl Worker + 'static) {
        self.workers.push(Arc::new(worker));
    }

    /// Add multiple workers
    pub fn add_workers(&mut self, workers: impl IntoIterator<Item = Arc<dyn Worker>>) {
        self.workers.extend(workers);
    }

    /// Register an event listener
    pub fn add_event_listener(&self, listener: Arc<dyn TaskRunnerEventsListener>) {
        self.event_dispatcher.register(listener);
    }

    /// Enable metrics collection and (optionally) the HTTP scrape endpoint.
    ///
    /// Creates a [`MetricsCollector`] with the given settings and wires it
    /// as both the [`TaskRunnerEventsListener`] (for task/workflow counters
    /// and histograms) and the
    /// [`HttpMetricsObserver`](crate::http::HttpMetricsObserver) (for
    /// `http_api_client_request_seconds`).
    ///
    /// Must be called **before** [`start`](Self::start). Clients vended
    /// after this call (via [`conductor_client`](Self::conductor_client),
    /// [`task_client`](Self::task_client), etc.) will share the observer.
    pub fn enable_metrics(&mut self, settings: MetricsSettings) {
        let collector = Arc::new(MetricsCollector::new(settings));
        self.event_dispatcher
            .register(collector.clone() as Arc<dyn TaskRunnerEventsListener>);
        self.api_client = ApiClient::with_http_observer(
            self.config.clone(),
            collector.clone() as Arc<dyn crate::http::HttpMetricsObserver>,
        )
        .expect("ApiClient creation should not fail on previously valid config");
        self.metrics_collector = Some(collector);
    }

    /// Get the metrics collector
    pub fn metrics_collector(&self) -> Option<&Arc<MetricsCollector>> {
        self.metrics_collector.as_ref()
    }

    /// Get the event dispatcher
    pub fn event_dispatcher(&self) -> &EventDispatcher {
        &self.event_dispatcher
    }

    /// Get a task client
    pub fn task_client(&self) -> TaskClient {
        TaskClient::new(self.api_client.clone())
    }

    /// Get a metadata client
    pub fn metadata_client(&self) -> MetadataClient {
        MetadataClient::new(self.api_client.clone())
    }

    /// Get the Conductor client wired to this handler's event dispatcher.
    ///
    /// Sharing the dispatcher means `WorkflowStarted` /
    /// `WorkflowStartFailure` events emitted by the returned client's
    /// `WorkflowClient` flow into the same `MetricsCollector` that
    /// [`enable_metrics`](Self::enable_metrics) installed.
    pub fn conductor_client(&self) -> ConductorClient {
        ConductorClient::from_api_client(self.api_client.clone())
            .with_event_dispatcher(self.event_dispatcher.clone())
    }

    /// Get a schema client
    pub fn schema_client(&self) -> SchemaClient {
        SchemaClient::new(self.api_client.clone())
    }

    /// Start all workers
    pub async fn start(&mut self) -> Result<()> {
        if self.workers.is_empty() {
            return Err(ConductorError::worker("No workers registered"));
        }

        info!(worker_count = self.workers.len(), "Starting task handler");

        // Start metrics HTTP server if configured
        if let Some(ref collector) = self.metrics_collector {
            self.metrics_handle = collector.start_http_server().await;
        }

        // Register task definitions if configured
        let metadata_client = self.metadata_client();
        for worker in &self.workers {
            // Resolve worker configuration
            let config = self.resolve_worker_config(worker.as_ref());

            // Register task definition if configured
            if config.register_task_def {
                if let Err(e) = self
                    .register_task_definition(&metadata_client, worker.as_ref(), &config)
                    .await
                {
                    warn!(
                        task_type = %worker.task_definition_name(),
                        error = %e,
                        "Failed to register task definition (worker will still start)"
                    );
                }
            }

            debug_log_worker(worker.as_ref(), &config);
        }

        // Create and start task runners
        let task_client = self.task_client();

        for worker in &self.workers {
            let runner = Arc::new(TaskRunner::new(
                Arc::clone(worker),
                task_client.clone(),
                self.event_dispatcher.clone(),
            ));

            self.runners.push(Arc::clone(&runner));

            let handle = tokio::spawn(async move {
                runner.run().await;
            });

            self.handles.push(handle);
        }

        info!(
            runner_count = self.runners.len(),
            "Task handler started successfully"
        );

        Ok(())
    }

    /// Stop all workers gracefully
    ///
    /// This method:
    /// 1. Signals all runners to stop polling
    /// 2. Waits for in-flight tasks to complete (up to 30 seconds)
    /// 3. Stops the metrics server
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping task handler");

        // Signal all runners to stop
        for runner in &self.runners {
            runner.stop();
        }

        // Wait for all runner handles to complete (with timeout)
        // The runners themselves wait for their spawned tasks
        let shutdown_timeout = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();

        // Drain handles using futures::future::join_all for cleaner handling
        let handles: Vec<_> = self.handles.drain(..).collect();

        if !handles.is_empty() {
            let wait_result =
                tokio::time::timeout(shutdown_timeout, futures::future::join_all(handles)).await;

            match wait_result {
                Ok(results) => {
                    for result in results {
                        if let Err(e) = result {
                            error!(error = %e, "Worker task panicked during shutdown");
                        }
                    }
                    debug!(
                        elapsed_ms = start.elapsed().as_millis(),
                        "All workers stopped gracefully"
                    );
                }
                Err(_) => {
                    warn!(
                        timeout_secs = shutdown_timeout.as_secs(),
                        "Timeout waiting for workers to stop, some tasks may still be running"
                    );
                }
            }
        }

        // Stop metrics server
        if let Some(handle) = self.metrics_handle.take() {
            handle.abort();
        }

        info!("Task handler stopped");
        Ok(())
    }

    /// Wait for all workers to complete
    pub async fn join(&mut self) -> Result<()> {
        for handle in self.handles.drain(..) {
            if let Err(e) = handle.await {
                error!(error = %e, "Worker task panicked");
            }
        }
        Ok(())
    }

    /// Get the number of registered workers
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Check if all workers are running
    pub fn is_running(&self) -> bool {
        !self.runners.is_empty() && self.runners.iter().all(|r| r.is_running())
    }

    /// Pause a specific worker by task type
    pub fn pause_worker(&self, task_type: &str) {
        for runner in &self.runners {
            if runner.task_type() == task_type {
                runner.pause();
            }
        }
    }

    /// Resume a specific worker by task type
    pub fn resume_worker(&self, task_type: &str) {
        for runner in &self.runners {
            if runner.task_type() == task_type {
                runner.resume();
            }
        }
    }

    /// Pause all workers
    pub fn pause_all(&self) {
        for runner in &self.runners {
            runner.pause();
        }
    }

    /// Resume all workers
    pub fn resume_all(&self) {
        for runner in &self.runners {
            runner.resume();
        }
    }

    /// Resolve worker configuration from environment variables
    fn resolve_worker_config(&self, worker: &dyn Worker) -> WorkerConfig {
        let defaults = WorkerConfig {
            task_definition_name: worker.task_definition_name().to_string(),
            poll_interval: std::time::Duration::from_millis(worker.poll_interval_millis()),
            domain: worker.domain().map(|s| s.to_string()),
            worker_id: worker.identity(),
            thread_count: worker.thread_count(),
            ..Default::default()
        };

        resolve_worker_config(worker.task_definition_name(), defaults)
    }

    /// Register a task definition for a worker
    async fn register_task_definition(
        &self,
        metadata_client: &MetadataClient,
        worker: &dyn Worker,
        config: &WorkerConfig,
    ) -> Result<()> {
        let task_name = worker.task_definition_name();

        // Check if task definition exists
        let exists = metadata_client.task_def_exists(task_name).await?;

        if exists && !config.overwrite_task_def {
            debug!(
                task_type = %task_name,
                "Task definition already exists, skipping registration (overwrite_task_def=false)"
            );
            return Ok(());
        }

        // Create task definition from worker
        let task_def =
            TaskDef::new(task_name).with_description("Task registered by Rust SDK worker");

        if exists {
            info!(
                task_type = %task_name,
                "Updating existing task definition"
            );
            metadata_client.update_task_def(&task_def).await?;
        } else {
            info!(
                task_type = %task_name,
                "Registering new task definition"
            );
            metadata_client.register_task_def(&task_def).await?;
        }

        // Register input/output schemas if provided
        self.register_worker_schemas(worker, task_name).await?;

        Ok(())
    }

    /// Register input and output schemas for a worker
    async fn register_worker_schemas(&self, worker: &dyn Worker, task_name: &str) -> Result<()> {
        let schema_client = self.schema_client();

        // Register input schema if provided
        if let Some(input_schema) = worker.input_schema() {
            let schema_def = SchemaDef::new(format!("{}_input", task_name), 1, input_schema);

            match schema_client.register_schema(&schema_def).await {
                Ok(()) => {
                    info!(
                        task_type = %task_name,
                        schema_name = %schema_def.name,
                        "Registered input schema"
                    );
                }
                Err(e) => {
                    warn!(
                        task_type = %task_name,
                        error = %e,
                        "Failed to register input schema (continuing anyway)"
                    );
                }
            }
        }

        // Register output schema if provided
        if let Some(output_schema) = worker.output_schema() {
            let schema_def = SchemaDef::new(format!("{}_output", task_name), 1, output_schema);

            match schema_client.register_schema(&schema_def).await {
                Ok(()) => {
                    info!(
                        task_type = %task_name,
                        schema_name = %schema_def.name,
                        "Registered output schema"
                    );
                }
                Err(e) => {
                    warn!(
                        task_type = %task_name,
                        error = %e,
                        "Failed to register output schema (continuing anyway)"
                    );
                }
            }
        }

        Ok(())
    }
}

fn debug_log_worker(worker: &dyn Worker, config: &WorkerConfig) {
    info!(
        task_type = %worker.task_definition_name(),
        worker_id = %config.worker_id,
        thread_count = config.thread_count,
        poll_interval_ms = config.poll_interval.as_millis(),
        domain = ?config.domain,
        paused = config.paused,
        register_task_def = config.register_task_def,
        "Conductor Worker registered"
    );
}

/// Builder for TaskHandler
pub struct TaskHandlerBuilder {
    config: Configuration,
    workers: Vec<Arc<dyn Worker>>,
    event_listeners: Vec<Arc<dyn TaskRunnerEventsListener>>,
    metrics_settings: Option<MetricsSettings>,
}

impl TaskHandlerBuilder {
    /// Create a new builder
    pub fn new(config: Configuration) -> Self {
        Self {
            config,
            workers: Vec::new(),
            event_listeners: Vec::new(),
            metrics_settings: None,
        }
    }

    /// Add a worker
    pub fn worker(mut self, worker: impl Worker + 'static) -> Self {
        self.workers.push(Arc::new(worker));
        self
    }

    /// Add multiple workers
    pub fn workers(mut self, workers: impl IntoIterator<Item = Arc<dyn Worker>>) -> Self {
        self.workers.extend(workers);
        self
    }

    /// Add an event listener
    pub fn event_listener(mut self, listener: impl TaskRunnerEventsListener + 'static) -> Self {
        self.event_listeners.push(Arc::new(listener));
        self
    }

    /// Enable metrics with the given settings
    pub fn metrics(mut self, settings: MetricsSettings) -> Self {
        self.metrics_settings = Some(settings);
        self
    }

    /// Build the task handler
    pub fn build(self) -> Result<TaskHandler> {
        let mut handler = TaskHandler::new(self.config)?;

        handler.add_workers(self.workers);

        for listener in self.event_listeners {
            handler.add_event_listener(listener);
        }

        if let Some(settings) = self.metrics_settings {
            handler.enable_metrics(settings);
        }

        Ok(handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::{Worker, WorkerOutput};
    use async_trait::async_trait;

    struct TestWorker {
        name: String,
    }

    #[async_trait]
    impl Worker for TestWorker {
        fn task_definition_name(&self) -> &str {
            &self.name
        }

        async fn execute(&self, _task: &crate::models::Task) -> Result<WorkerOutput> {
            Ok(WorkerOutput::completed_with_result("test"))
        }
    }

    #[test]
    fn test_task_handler_builder() {
        let config = Configuration::new("http://localhost:8080/api");

        let handler = TaskHandlerBuilder::new(config)
            .worker(TestWorker {
                name: "task1".to_string(),
            })
            .worker(TestWorker {
                name: "task2".to_string(),
            })
            .build();

        assert!(handler.is_ok());
        let handler = handler.unwrap();
        assert_eq!(handler.worker_count(), 2);
    }
}
