//! Configuration module for Conductor SDK
//!
//! Provides hierarchical configuration with support for:
//! - Environment variables
//! - Code-level defaults
//! - Worker-specific overrides

mod settings;
mod worker_config;

pub use settings::Configuration;
pub use worker_config::{resolve_worker_config, WorkerConfig};
