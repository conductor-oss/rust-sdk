//! Event-driven system for observability and extensibility
//!
//! This module provides an event-driven architecture similar to the Python SDK,
//! allowing for decoupled metrics collection and custom monitoring.

mod dispatcher;
mod task_runner_events;

pub use dispatcher::{EventDispatcher, SyncEventDispatcher};
pub use task_runner_events::{
    PollCompleted, PollFailure, PollStarted, TaskExecutionCompleted, TaskExecutionFailure,
    TaskExecutionStarted, TaskRunnerEvent, TaskRunnerEventsListener, TaskUpdateFailure,
};
