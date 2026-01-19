//! Worker trait and function types
//!
//! This module provides:
//! - `Worker` trait for implementing task workers
//! - `FnWorker` for simple closure-based workers (clones Task)
//! - `FnWorkerArc` for high-performance workers (uses Arc<Task>)

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;
use crate::models::{Task, TaskInProgress, TaskResult, TaskResultStatus};

/// Output type for worker execution
#[derive(Debug, Clone)]
pub enum WorkerOutput {
    /// Task completed successfully with output
    Completed(HashMap<String, Value>),

    /// Task failed with an error message
    Failed(String),

    /// Task is still in progress (for long-running tasks)
    InProgress(TaskInProgress),
}

impl WorkerOutput {
    /// Create a completed output with a single result value
    pub fn completed_with_result(result: impl serde::Serialize) -> Self {
        let mut output = HashMap::new();
        output.insert(
            "result".to_string(),
            serde_json::to_value(result).unwrap_or(Value::Null),
        );
        WorkerOutput::Completed(output)
    }

    /// Create a completed output with a map
    pub fn completed(output: HashMap<String, Value>) -> Self {
        WorkerOutput::Completed(output)
    }

    /// Create a completed output with no data
    pub fn complete() -> Self {
        WorkerOutput::Completed(HashMap::new())
    }

    /// Create a failed output
    pub fn failed(reason: impl Into<String>) -> Self {
        WorkerOutput::Failed(reason.into())
    }

    /// Create an in-progress output
    pub fn in_progress(callback_after_seconds: i64) -> Self {
        WorkerOutput::InProgress(TaskInProgress::new(callback_after_seconds))
    }

    /// Convert to TaskResult
    pub fn into_task_result(self, task: &Task, worker_id: &str) -> TaskResult {
        match self {
            WorkerOutput::Completed(output) => TaskResult {
                task_id: task.task_id.clone(),
                workflow_instance_id: task.workflow_instance_id.clone(),
                worker_id: Some(worker_id.to_string()),
                status: TaskResultStatus::Completed,
                output_data: output,
                ..Default::default()
            },
            WorkerOutput::Failed(reason) => TaskResult {
                task_id: task.task_id.clone(),
                workflow_instance_id: task.workflow_instance_id.clone(),
                worker_id: Some(worker_id.to_string()),
                status: TaskResultStatus::Failed,
                reason_for_incompletion: Some(reason),
                ..Default::default()
            },
            WorkerOutput::InProgress(tip) => TaskResult {
                task_id: task.task_id.clone(),
                workflow_instance_id: task.workflow_instance_id.clone(),
                worker_id: Some(worker_id.to_string()),
                status: TaskResultStatus::InProgress,
                callback_after_seconds: tip.callback_after_seconds,
                output_data: tip.output,
                ..Default::default()
            },
        }
    }
}

/// Trait for implementing Conductor task workers
#[async_trait]
pub trait Worker: Send + Sync {
    /// Get the task definition name this worker handles
    fn task_definition_name(&self) -> &str;

    /// Execute the task and return the output
    async fn execute(&self, task: &Task) -> Result<WorkerOutput>;

    /// Get the worker identity (optional, defaults to hostname-pid)
    fn identity(&self) -> String {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "unknown".to_string());
        format!("{}-{}", hostname, std::process::id())
    }

    /// Get the domain for this worker (optional)
    fn domain(&self) -> Option<&str> {
        None
    }

    /// Get the poll interval in milliseconds
    fn poll_interval_millis(&self) -> u64 {
        100
    }

    /// Get the maximum concurrent task executions
    fn thread_count(&self) -> usize {
        1
    }

    /// Get the input JSON Schema for this worker (optional)
    ///
    /// Return a JSON Schema that describes the expected input format.
    /// Used when `register_task_def` is enabled to register input schema.
    fn input_schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Get the output JSON Schema for this worker (optional)
    ///
    /// Return a JSON Schema that describes the output format.
    /// Used when `register_task_def` is enabled to register output schema.
    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Type alias for async worker functions
pub type WorkerFn =
    Arc<dyn Fn(Task) -> Pin<Box<dyn Future<Output = Result<WorkerOutput>> + Send>> + Send + Sync>;

/// Simple function-based worker implementation
pub struct FnWorker {
    task_name: String,
    execute_fn: WorkerFn,
    identity: String,
    domain: Option<String>,
    poll_interval_millis: u64,
    thread_count: usize,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
}

impl FnWorker {
    /// Create a new function-based worker
    pub fn new<F, Fut>(task_name: impl Into<String>, f: F) -> Self
    where
        F: Fn(Task) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<WorkerOutput>> + Send + 'static,
    {
        let task_name = task_name.into();
        let identity = format!(
            "{}-{}",
            hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string()),
            std::process::id()
        );

        Self {
            task_name,
            execute_fn: Arc::new(move |task| Box::pin(f(task))),
            identity,
            domain: None,
            poll_interval_millis: 100,
            thread_count: 1,
            input_schema: None,
            output_schema: None,
        }
    }

    /// Set the worker domain
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Set the poll interval
    pub fn with_poll_interval_millis(mut self, millis: u64) -> Self {
        self.poll_interval_millis = millis;
        self
    }

    /// Set the thread count
    pub fn with_thread_count(mut self, count: usize) -> Self {
        self.thread_count = count;
        self
    }

    /// Set the worker identity
    pub fn with_identity(mut self, identity: impl Into<String>) -> Self {
        self.identity = identity.into();
        self
    }

    /// Set the input JSON Schema
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Set the output JSON Schema
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set input schema from a JsonSchema type
    pub fn with_input_schema_from<T: schemars::JsonSchema>(mut self, strict: bool) -> Self {
        self.input_schema = Some(crate::schema::generate_schema::<T>(strict));
        self
    }

    /// Set output schema from a JsonSchema type
    pub fn with_output_schema_from<T: schemars::JsonSchema>(mut self, strict: bool) -> Self {
        self.output_schema = Some(crate::schema::generate_schema::<T>(strict));
        self
    }
}

#[async_trait]
impl Worker for FnWorker {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        (self.execute_fn)(task.clone()).await
    }

    fn identity(&self) -> String {
        self.identity.clone()
    }

    fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    fn poll_interval_millis(&self) -> u64 {
        self.poll_interval_millis
    }

    fn thread_count(&self) -> usize {
        self.thread_count
    }

    fn input_schema(&self) -> Option<serde_json::Value> {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        self.output_schema.clone()
    }
}

/// Type alias for async worker functions that use Arc<Task> (zero-copy)
pub type WorkerFnArc = Arc<
    dyn Fn(Arc<Task>) -> Pin<Box<dyn Future<Output = Result<WorkerOutput>> + Send>> + Send + Sync,
>;

/// High-performance function-based worker that uses Arc<Task> to avoid cloning
///
/// Use this instead of `FnWorker` for high-throughput scenarios where task
/// cloning overhead is significant.
///
/// # Example
/// ```rust,ignore
/// let worker = FnWorkerArc::new("my_task", |task: Arc<Task>| async move {
///     let name = task.get_input_string("name").unwrap_or_default();
///     Ok(WorkerOutput::completed_with_result(format!("Hello, {}!", name)))
/// });
/// ```
pub struct FnWorkerArc {
    task_name: String,
    execute_fn: WorkerFnArc,
    identity: String,
    domain: Option<String>,
    poll_interval_millis: u64,
    thread_count: usize,
    input_schema: Option<serde_json::Value>,
    output_schema: Option<serde_json::Value>,
}

impl FnWorkerArc {
    /// Create a new high-performance function-based worker
    pub fn new<F, Fut>(task_name: impl Into<String>, f: F) -> Self
    where
        F: Fn(Arc<Task>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<WorkerOutput>> + Send + 'static,
    {
        let task_name = task_name.into();
        let identity = format!(
            "{}-{}",
            hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string()),
            std::process::id()
        );

        Self {
            task_name,
            execute_fn: Arc::new(move |task| Box::pin(f(task))),
            identity,
            domain: None,
            poll_interval_millis: 100,
            thread_count: 1,
            input_schema: None,
            output_schema: None,
        }
    }

    /// Set the worker domain
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Set the poll interval
    pub fn with_poll_interval_millis(mut self, millis: u64) -> Self {
        self.poll_interval_millis = millis;
        self
    }

    /// Set the thread count
    pub fn with_thread_count(mut self, count: usize) -> Self {
        self.thread_count = count;
        self
    }

    /// Set the worker identity
    pub fn with_identity(mut self, identity: impl Into<String>) -> Self {
        self.identity = identity.into();
        self
    }

    /// Set the input JSON Schema
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }

    /// Set the output JSON Schema
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set input schema from a JsonSchema type
    pub fn with_input_schema_from<T: schemars::JsonSchema>(mut self, strict: bool) -> Self {
        self.input_schema = Some(crate::schema::generate_schema::<T>(strict));
        self
    }

    /// Set output schema from a JsonSchema type
    pub fn with_output_schema_from<T: schemars::JsonSchema>(mut self, strict: bool) -> Self {
        self.output_schema = Some(crate::schema::generate_schema::<T>(strict));
        self
    }

    /// Execute with Arc<Task> directly (internal use)
    pub async fn execute_arc(&self, task: Arc<Task>) -> Result<WorkerOutput> {
        (self.execute_fn)(task).await
    }
}

#[async_trait]
impl Worker for FnWorkerArc {
    fn task_definition_name(&self) -> &str {
        &self.task_name
    }

    async fn execute(&self, task: &Task) -> Result<WorkerOutput> {
        // Still need to clone for the trait interface, but users can use execute_arc directly
        (self.execute_fn)(Arc::new(task.clone())).await
    }

    fn identity(&self) -> String {
        self.identity.clone()
    }

    fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    fn poll_interval_millis(&self) -> u64 {
        self.poll_interval_millis
    }

    fn thread_count(&self) -> usize {
        self.thread_count
    }

    fn input_schema(&self) -> Option<serde_json::Value> {
        self.input_schema.clone()
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        self.output_schema.clone()
    }
}

/// Macro to simplify worker creation from closures
///
/// # Example
/// ```rust,ignore
/// let worker = worker_fn!("my_task", |task| async move {
///     Ok(WorkerOutput::completed_with_result("done"))
/// });
/// ```
#[macro_export]
macro_rules! worker_fn {
    ($task_name:expr, $handler:expr) => {
        $crate::worker::FnWorker::new($task_name, $handler)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fn_worker() {
        let worker = FnWorker::new("test_task", |task: Task| async move {
            let name = task
                .get_input_string("name")
                .unwrap_or_else(|| "World".to_string());
            Ok(WorkerOutput::completed_with_result(format!(
                "Hello, {}!",
                name
            )))
        })
        .with_thread_count(5)
        .with_poll_interval_millis(500);

        assert_eq!(worker.task_definition_name(), "test_task");
        assert_eq!(worker.thread_count(), 5);
        assert_eq!(worker.poll_interval_millis(), 500);

        let mut task = Task::default();
        task.input_data
            .insert("name".to_string(), serde_json::json!("Rust"));

        let result = worker.execute(&task).await.unwrap();
        match result {
            WorkerOutput::Completed(output) => {
                assert!(output.contains_key("result"));
            }
            _ => panic!("Expected completed output"),
        }
    }

    #[test]
    fn test_worker_output_conversion() {
        let task = Task {
            task_id: "task-1".to_string(),
            workflow_instance_id: "wf-1".to_string(),
            ..Default::default()
        };

        let output = WorkerOutput::completed_with_result("success");
        let result = output.into_task_result(&task, "worker-1");

        assert_eq!(result.task_id, "task-1");
        assert_eq!(result.status, TaskResultStatus::Completed);
        assert_eq!(result.worker_id, Some("worker-1".to_string()));
    }
}
