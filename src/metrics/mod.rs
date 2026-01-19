//! Prometheus metrics collection for Conductor workers
//!
//! This module provides metrics collection using the event-driven architecture.

mod collector;
mod settings;

pub use collector::MetricsCollector;
pub use settings::MetricsSettings;
