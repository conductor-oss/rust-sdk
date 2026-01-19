//! High-level Conductor API clients
//!
//! This module provides typed clients for interacting with Conductor:
//! - `TaskClient`: Task polling and updates
//! - `WorkflowClient`: Workflow execution management
//! - `MetadataClient`: Workflow and task definitions
//! - `SchedulerClient`: Workflow scheduling (cron-based)
//! - `SecretClient`: Secret management
//! - `AuthorizationClient`: Users, groups, and permissions
//! - `IntegrationClient`: External system integrations
//! - `PromptClient`: AI prompt templates
//! - `SchemaClient`: Schema definitions
//! - `EventClient`: Event queue configurations
//! - `ConductorClient` / `OrkesClients`: Combined client for all operations

mod authorization_client;
mod conductor_client;
mod event_client;
mod integration_client;
mod metadata_client;
mod orkes_metadata_client;
mod prompt_client;
mod scheduler_client;
mod schema_client;
mod secret_client;
mod task_client;
mod workflow_client;

pub use authorization_client::AuthorizationClient;
pub use conductor_client::{ConductorClient, ConductorClientBuilder};
pub use event_client::{EventClient, QueueConfiguration};
pub use integration_client::IntegrationClient;
pub use metadata_client::MetadataClient;
pub use orkes_metadata_client::OrkesMetadataClient;
pub use prompt_client::PromptClient;
pub use scheduler_client::SchedulerClient;
pub use schema_client::SchemaClient;
pub use secret_client::SecretClient;
pub use task_client::TaskClient;
pub use workflow_client::{
    CorrelationIdsSearchRequest, SearchResult, SignalResponse, TestWorkflowRequest, WorkflowClient,
    WorkflowRun, WorkflowStateUpdate,
};

/// Alias for ConductorClient, matching the Python SDK's OrkesClients
pub type OrkesClients = ConductorClient;
