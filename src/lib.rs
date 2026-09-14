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

#[cfg(feature = "agents")]
pub mod agents;

// Re-exports for convenience
#[cfg(feature = "agents")]
pub use agents::{
    agent_run_span, clear_mcp_discovery_cache, compile_span, create_skill_workers,
    discover_mcp_tools, expand_mcp_tool_def, format_prompt_with_params, handoff_span,
    is_tracing_enabled, llm_call_span, load_skill, load_skills, ocg_agent, ocg_tools, plan_execute,
    record_token_usage, tool_call_span, traced_agent_run, traced_tool_call, Action,
    AgentConfigSerializer, AgentDef, AgentEvent, AgentHandle, AgentResult, AgentRuntime,
    AgentStatus, AgentStream, CallbackContext, CallbackHandler, CliConfig, CodeExecutionConfig,
    CodeExecutor, CommandValidator, ConfiguredExecutor, Context, ConversationMemory, Credentials,
    DiscoveredMcpTool, DockerCodeExecutor, ExecutionResult, FunctionGuardrail, GateCondition,
    GateHandler, Generate, Guardrail, GuardrailCheck, GuardrailResult, InMemoryStore, LlmGuardrail,
    LocalCodeExecutor, MemoryEntry, MemoryStore, Message, MessageRole, OcgAgentOptions,
    OcgToolSelection, OnFail, Op, OpBody, Plan, PlanExecuteOptions, Position, PrefillToolCall, Ref,
    RegexGuardrail, RegexMode, RunSettings, SemanticMemory, ServerlessCodeExecutor, SkillAgent,
    SkillOptions, Step, StopWhenHandler, Strategy, SwarmConditionFn, SwarmContext, SwarmTransition,
    TerminationCondition, TextGate, ToolCall, ToolContext, ToolDef, ToolHandler, ToolType,
    Validation, OCG_SYSTEM_PROMPT,
};
#[cfg(feature = "agents")]
pub use client::AgentClient;
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
#[cfg(feature = "agents")]
pub use conductor_macros::tool;
#[cfg(feature = "macros")]
pub use conductor_macros::{worker, worker_task};
