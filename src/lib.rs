// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

pub mod client;
pub mod configuration;
pub mod error;
pub mod events;
pub mod http;
pub mod metrics;
pub mod models;
pub mod schema;
pub mod worker;

// Re-exports for convenience
pub use client::{
    AuthorizationClient, ConductorClient, EventClient, IntegrationClient, MetadataClient,
    OrkesClients, PromptClient, QueueConfiguration, SchedulerClient, SchemaClient, SecretClient,
    TaskClient, WorkflowClient,
};
pub use configuration::{Configuration, WorkerConfig};
pub use error::{ConductorError, Result};
pub use events::{EventDispatcher, TaskRunnerEvent, TaskRunnerEventsListener};
pub use metrics::{MetricsCollector, MetricsSettings};
pub use models::{
    AccessKey, AccessType, ChatMessage, ConductorApplication, ConductorUser,
    CreateOrUpdateApplicationRequest, CreatedAccessKey, EmbeddedTaskDef, GrantedPermission, Group,
    Integration, IntegrationApi, IntegrationApiUpdate, IntegrationUpdate, MetadataTag, Permission,
    PromptTemplate, Role, SaveScheduleRequest, SchemaDef, SearchResultWorkflowScheduleExecution,
    StartWorkflowRequest, StartWorkflowScheduleRequest, StateChangeConfig, StateChangeEvent,
    StateChangeEventType, SubWorkflowParams, SubjectRef, SubjectType, TargetRef, TargetType, Task,
    TaskDef, TaskInProgress, TaskResult, TaskResultStatus, TaskStatus, TaskType,
    UpsertGroupRequest, UpsertUserRequest, Workflow, WorkflowDef, WorkflowSchedule,
    WorkflowScheduleExecution, WorkflowStatus, WorkflowTask, WorkflowTimeoutPolicy,
};
pub use worker::{
    FnWorker, TaskContext, TaskHandler, TaskRunner, Worker, WorkerHost, WorkerOutput,
};

// Re-export the procedural macros when the feature is enabled
#[cfg(feature = "macros")]
pub use conductor_macros::{worker, worker_task};
