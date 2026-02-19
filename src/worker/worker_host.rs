// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::info;

use crate::configuration::Configuration;
use crate::error::Result;
use crate::events::TaskRunnerEventsListener;
use crate::metrics::MetricsSettings;

use super::{TaskHandler, Worker};

/// High-level worker host for running Conductor workers
///
/// # Example
///
/// ```rust,no_run
/// use conductor::{WorkerHost, Configuration, MetricsSettings, worker::FnWorker, worker::WorkerOutput};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let host = WorkerHost::builder(Configuration::default())
///         .worker(FnWorker::new("greet", |task| async move {
///             let name = task.get_input_string("name").unwrap_or_default();
///             Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", name)))
///         }))
///         .with_metrics(MetricsSettings::default().with_http_port(9090))
///         .start()
///         .await?;
///
///     // Run until Ctrl+C
///     host.wait_for_shutdown().await?;
///
///     Ok(())
/// }
/// ```
pub struct WorkerHost {
    handler: TaskHandler,
    shutdown_rx: Option<oneshot::Receiver<()>>,
}

impl WorkerHost {
    /// Create a new worker host builder
    pub fn builder(config: Configuration) -> WorkerHostBuilder {
        WorkerHostBuilder::new(config)
    }

    /// Wait for shutdown signal (Ctrl+C)
    pub async fn wait_for_shutdown(mut self) -> Result<()> {
        info!("Worker host running, press Ctrl+C to stop");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
            _ = async {
                if let Some(rx) = self.shutdown_rx.take() {
                    let _ = rx.await;
                } else {
                    std::future::pending::<()>().await
                }
            } => {
                info!("Received shutdown request");
            }
        }

        self.handler.stop().await
    }

    /// Stop the worker host
    pub async fn stop(mut self) -> Result<()> {
        self.handler.stop().await
    }

    /// Get a reference to the underlying task handler
    pub fn handler(&self) -> &TaskHandler {
        &self.handler
    }

    /// Get a mutable reference to the underlying task handler
    pub fn handler_mut(&mut self) -> &mut TaskHandler {
        &mut self.handler
    }
}

/// Builder for WorkerHost
pub struct WorkerHostBuilder {
    config: Configuration,
    workers: Vec<Arc<dyn Worker>>,
    event_listeners: Vec<Arc<dyn TaskRunnerEventsListener>>,
    metrics_settings: Option<MetricsSettings>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
}

impl WorkerHostBuilder {
    /// Create a new builder
    pub fn new(config: Configuration) -> Self {
        Self {
            config,
            workers: Vec::new(),
            event_listeners: Vec::new(),
            metrics_settings: None,
            shutdown_rx: None,
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
    pub fn with_metrics(mut self, settings: MetricsSettings) -> Self {
        self.metrics_settings = Some(settings);
        self
    }

    /// Set up programmatic shutdown
    pub fn with_shutdown_channel(self) -> (Self, oneshot::Sender<()>) {
        let (tx, rx) = oneshot::channel();
        (
            Self {
                config: self.config,
                workers: self.workers,
                event_listeners: self.event_listeners,
                metrics_settings: self.metrics_settings,
                shutdown_rx: Some(rx),
            },
            tx,
        )
    }

    /// Start the worker host
    pub async fn start(self) -> Result<WorkerHost> {
        let mut handler = TaskHandler::new(self.config)?;

        handler.add_workers(self.workers);

        for listener in self.event_listeners {
            handler.add_event_listener(listener);
        }

        if let Some(settings) = self.metrics_settings {
            handler.enable_metrics(settings);
        }

        handler.start().await?;

        // Use provided shutdown receiver or create a new one (that will never fire)
        let shutdown_rx = self.shutdown_rx.or_else(|| {
            let (_tx, rx) = oneshot::channel();
            Some(rx)
        });

        Ok(WorkerHost {
            handler,
            shutdown_rx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Task;
    use crate::worker::{FnWorker, WorkerOutput};

    #[tokio::test]
    async fn test_worker_host_builder() {
        let config = Configuration::new("http://localhost:8080/api");

        let builder = WorkerHostBuilder::new(config)
            .worker(FnWorker::new("test", |_: Task| async {
                Ok(WorkerOutput::completed_with_result("test"))
            }));

        assert_eq!(builder.workers.len(), 1);
    }
}
