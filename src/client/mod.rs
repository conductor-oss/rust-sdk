// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

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
