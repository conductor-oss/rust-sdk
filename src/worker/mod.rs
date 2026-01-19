//! Worker framework for executing Conductor tasks
//!
//! This module provides the core worker infrastructure:
//! - `Worker`: Trait for implementing task workers
//! - `FnWorker`: Function-based worker (clones task for each execution)
//! - `FnWorkerArc`: High-performance worker using Arc<Task> (zero-copy)
//! - `TaskRunner`: Async task polling and execution loop
//! - `TaskHandler`: Manages multiple workers and their lifecycle
//! - `WorkerHost`: High-level worker hosting
//! - `TaskContext`: Execution context for accessing task metadata

mod task_context;
mod task_handler;
mod task_runner;
mod worker_host;
mod worker_trait;

pub use task_context::TaskContext;
pub use task_handler::{TaskHandler, TaskHandlerBuilder};
pub use task_runner::TaskRunner;
pub use worker_host::WorkerHost;
pub use worker_trait::{FnWorker, FnWorkerArc, Worker, WorkerFn, WorkerFnArc, WorkerOutput};
