//! Workflow definition model

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Workflow timeout policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowTimeoutPolicy {
    /// Timeout the workflow
    #[default]
    TimeOutWf,
    /// Alert only
    AlertOnly,
}

/// Task type in a workflow
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskType {
    /// Simple task executed by a worker
    #[default]
    Simple,
    /// Dynamic task
    Dynamic,
    /// Fork task for parallel execution
    ForkJoin,
    /// Fork dynamic task
    ForkJoinDynamic,
    /// Decision task (deprecated, use Switch)
    Decision,
    /// Switch task for branching
    Switch,
    /// Join task
    Join,
    /// Do-while loop task
    DoWhile,
    /// Subworkflow task
    SubWorkflow,
    /// Event task
    Event,
    /// Wait task
    Wait,
    /// Human task
    Human,
    /// User defined task
    UserDefined,
    /// HTTP task
    Http,
    /// HTTP Poll task
    #[serde(rename = "HTTP_POLL")]
    HttpPoll,
    /// Lambda task
    Lambda,
    /// Inline task (JavaScript)
    Inline,
    /// Exclusive join task
    ExclusiveJoin,
    /// Terminate task
    Terminate,
    /// Kafka publish task
    KafkaPublish,
    /// JSON JQ transform task
    JsonJqTransform,
    /// Set variable task
    SetVariable,
    /// Start workflow task
    StartWorkflow,
    /// Wait for webhook task
    #[serde(rename = "WAIT_FOR_WEBHOOK")]
    WaitForWebhook,
    /// LLM text complete task
    #[serde(rename = "LLM_TEXT_COMPLETE")]
    LlmTextComplete,
    /// LLM chat complete task
    #[serde(rename = "LLM_CHAT_COMPLETE")]
    LlmChatComplete,
    /// LLM generate embeddings task
    #[serde(rename = "LLM_GENERATE_EMBEDDINGS")]
    LlmGenerateEmbeddings,
    /// LLM index text task
    #[serde(rename = "LLM_INDEX_TEXT")]
    LlmIndexText,
    /// LLM index document task
    #[serde(rename = "LLM_INDEX_DOCUMENT")]
    LlmIndexDocument,
    /// LLM search index task
    #[serde(rename = "LLM_SEARCH_INDEX")]
    LlmSearchIndex,
    /// LLM query embeddings task
    #[serde(rename = "LLM_GET_EMBEDDINGS")]
    LlmGetEmbeddings,
    /// LLM store embeddings task
    #[serde(rename = "LLM_STORE_EMBEDDINGS")]
    LlmStoreEmbeddings,
    /// Get document task
    #[serde(rename = "GET_DOCUMENT")]
    GetDocument,
}

/// A task definition within a workflow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTask {
    /// Task name
    pub name: String,

    /// Task reference name (unique within workflow)
    pub task_reference_name: String,

    /// Task type
    #[serde(rename = "type")]
    pub task_type: TaskType,

    /// Input parameters
    #[serde(default)]
    pub input_parameters: HashMap<String, serde_json::Value>,

    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Dynamic task name (for dynamic tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_task_name_param: Option<String>,

    /// Case value (for switch tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_value_param: Option<String>,

    /// Case expression (for switch tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_expression: Option<String>,

    /// Script expression (for inline tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_expression: Option<String>,

    /// Decision cases (for switch tasks)
    #[serde(default)]
    pub decision_cases: HashMap<String, Vec<WorkflowTask>>,

    /// Dynamic fork tasks param (for fork_join_dynamic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_fork_tasks_param: Option<String>,

    /// Dynamic fork tasks input param name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_fork_tasks_input_param_name: Option<String>,

    /// Default case (for switch tasks)
    #[serde(default)]
    pub default_case: Vec<WorkflowTask>,

    /// Fork tasks (for fork_join)
    #[serde(default)]
    pub fork_tasks: Vec<Vec<WorkflowTask>>,

    /// Start delay seconds
    #[serde(default)]
    pub start_delay: i32,

    /// Subworkflow param (for subworkflow tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_workflow_param: Option<SubWorkflowParams>,

    /// Join on (for join tasks)
    #[serde(default)]
    pub join_on: Vec<String>,

    /// Sink (for event tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink: Option<String>,

    /// Optional task flag
    #[serde(default)]
    pub optional: bool,

    /// Async complete flag (for wait tasks)
    #[serde(default)]
    pub async_complete: bool,

    /// Default exclusive join task (for exclusive_join)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_exclusive_join_task: Option<Vec<String>>,

    /// Loop condition (for do_while)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_condition: Option<String>,

    /// Loop over (for do_while)
    #[serde(default)]
    pub loop_over: Vec<WorkflowTask>,

    /// Retry count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<i32>,

    /// Evaluator type (for inline tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluator_type: Option<String>,

    /// Expression (for inline tasks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,

    /// Rate limited flag
    #[serde(default)]
    pub rate_limited: bool,

    /// Join on script (for join tasks with custom script)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_on_script: Option<String>,

    /// State change configuration (for audit events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_state_change: Option<StateChangeConfig>,

    /// Inline task definition (for embedded task defs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_definition: Option<EmbeddedTaskDef>,
}

/// Configuration for state change events on tasks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChangeConfig {
    /// Event types to trigger on
    #[serde(default)]
    pub event_type: Vec<StateChangeEventType>,

    /// Events to dispatch
    #[serde(default)]
    pub events: Vec<StateChangeEvent>,
}

impl StateChangeConfig {
    /// Create a new state change config
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event type
    pub fn on_event(mut self, event_type: StateChangeEventType) -> Self {
        self.event_type.push(event_type);
        self
    }

    /// Add an event to dispatch
    pub fn with_event(mut self, event: StateChangeEvent) -> Self {
        self.events.push(event);
        self
    }
}

/// State change event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateChangeEventType {
    /// On task scheduled
    OnScheduled,
    /// On task started
    OnStart,
    /// On task failed
    OnFailed,
    /// On task completed
    OnCompleted,
    /// On task cancelled
    OnCancelled,
}

/// Event to dispatch on state change
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChangeEvent {
    /// Task type to dispatch (task name)
    #[serde(rename = "type")]
    pub event_type: String,

    /// Payload for the task
    #[serde(default)]
    pub payload: HashMap<String, serde_json::Value>,
}

impl StateChangeEvent {
    /// Create a new state change event
    pub fn new(task_type: impl Into<String>) -> Self {
        Self {
            event_type: task_type.into(),
            payload: HashMap::new(),
        }
    }

    /// Add a payload parameter
    pub fn with_payload(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.payload.insert(key.into(), value.into());
        self
    }
}

/// Embedded task definition (for inline task defs in workflow tasks)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedTaskDef {
    /// Task name
    pub name: String,

    /// Retry count
    #[serde(default)]
    pub retry_count: i32,

    /// Timeout seconds
    #[serde(default)]
    pub timeout_seconds: i64,

    /// Response timeout seconds
    #[serde(default)]
    pub response_timeout_seconds: i64,
}

impl EmbeddedTaskDef {
    /// Create a new embedded task def
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set retry count
    pub fn with_retry_count(mut self, count: i32) -> Self {
        self.retry_count = count;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, seconds: i64) -> Self {
        self.timeout_seconds = seconds;
        self
    }
}

impl WorkflowTask {
    /// Create a simple task
    pub fn simple(name: impl Into<String>, task_ref_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Simple,
            ..Default::default()
        }
    }

    /// Create a subworkflow task
    pub fn sub_workflow(
        task_ref_name: impl Into<String>,
        workflow_name: impl Into<String>,
    ) -> Self {
        Self {
            name: "sub_workflow".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::SubWorkflow,
            sub_workflow_param: Some(SubWorkflowParams {
                name: workflow_name.into(),
                version: None,
                task_to_domain: HashMap::new(),
            }),
            ..Default::default()
        }
    }

    /// Create a wait task
    pub fn wait(task_ref_name: impl Into<String>) -> Self {
        Self {
            name: "wait".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Wait,
            ..Default::default()
        }
    }

    /// Create an HTTP task
    pub fn http(task_ref_name: impl Into<String>, uri: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert("uri".to_string(), serde_json::Value::String(uri.into()));
        input.insert(
            "method".to_string(),
            serde_json::Value::String("GET".to_string()),
        );

        Self {
            name: "http".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Http,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create an inline JavaScript task
    pub fn inline(task_ref_name: impl Into<String>, script: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "expression".to_string(),
            serde_json::Value::String(script.into()),
        );
        input.insert(
            "evaluatorType".to_string(),
            serde_json::Value::String("graaljs".to_string()),
        );

        Self {
            name: "inline".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Inline,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set input parameters
    pub fn with_input(mut self, input: HashMap<String, serde_json::Value>) -> Self {
        self.input_parameters = input;
        self
    }

    /// Add a single input parameter
    pub fn with_input_param(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.input_parameters.insert(
            key.into(),
            serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        );
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as optional
    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    /// Create a switch (decision) task with JavaScript expression
    pub fn switch(task_ref_name: impl Into<String>, case_expression: impl Into<String>) -> Self {
        Self {
            name: "switch".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Switch,
            expression: Some(case_expression.into()),
            evaluator_type: Some("javascript".to_string()),
            ..Default::default()
        }
    }

    /// Create a switch (decision) task with value_param evaluator
    ///
    /// Use this for simple value expressions like ${workflow.input.choice}
    /// The evaluator_type is set to "value-param" which directly evaluates the expression.
    pub fn switch_value_param(
        task_ref_name: impl Into<String>,
        case_expression: impl Into<String>,
    ) -> Self {
        Self {
            name: "switch".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Switch,
            expression: Some(case_expression.into()),
            evaluator_type: Some("value-param".to_string()),
            ..Default::default()
        }
    }

    /// Add a switch case
    pub fn with_switch_case(
        mut self,
        case_value: impl Into<String>,
        tasks: Vec<WorkflowTask>,
    ) -> Self {
        self.decision_cases.insert(case_value.into(), tasks);
        self
    }

    /// Set default case for switch
    pub fn with_default_case(mut self, tasks: Vec<WorkflowTask>) -> Self {
        self.default_case = tasks;
        self
    }

    /// Create a fork task for parallel execution
    pub fn fork(task_ref_name: impl Into<String>, branches: Vec<Vec<WorkflowTask>>) -> Self {
        Self {
            name: "fork".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::ForkJoin,
            fork_tasks: branches,
            ..Default::default()
        }
    }

    /// Create a join task
    pub fn join(task_ref_name: impl Into<String>, join_on: Vec<String>) -> Self {
        Self {
            name: "join".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Join,
            join_on,
            ..Default::default()
        }
    }

    /// Create a set variable task
    pub fn set_variable(task_ref_name: impl Into<String>) -> Self {
        Self {
            name: "set_variable".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::SetVariable,
            ..Default::default()
        }
    }

    /// Create a terminate task
    pub fn terminate(task_ref_name: impl Into<String>, status: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "terminationStatus".to_string(),
            serde_json::Value::String(status.into()),
        );

        Self {
            name: "terminate".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Terminate,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set termination reason
    pub fn with_termination_reason(mut self, reason: impl Into<String>) -> Self {
        self.input_parameters.insert(
            "terminationReason".to_string(),
            serde_json::Value::String(reason.into()),
        );
        self
    }

    /// Create a JSON JQ transform task
    pub fn json_jq_transform(task_ref_name: impl Into<String>, query: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "queryExpression".to_string(),
            serde_json::Value::String(query.into()),
        );

        Self {
            name: "json_jq_transform".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::JsonJqTransform,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create a wait task with duration
    pub fn wait_duration(task_ref_name: impl Into<String>, duration: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "duration".to_string(),
            serde_json::Value::String(duration.into()),
        );

        Self {
            name: "wait".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Wait,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create an event task
    pub fn event(task_ref_name: impl Into<String>, sink: impl Into<String>) -> Self {
        Self {
            name: "event".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Event,
            sink: Some(sink.into()),
            ..Default::default()
        }
    }

    /// Create a do-while loop task
    pub fn do_while(
        task_ref_name: impl Into<String>,
        loop_condition: impl Into<String>,
        loop_tasks: Vec<WorkflowTask>,
    ) -> Self {
        Self {
            name: "do_while".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::DoWhile,
            loop_condition: Some(loop_condition.into()),
            loop_over: loop_tasks,
            ..Default::default()
        }
    }

    /// Create a human task
    pub fn human(task_ref_name: impl Into<String>) -> Self {
        Self {
            name: "human".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Human,
            ..Default::default()
        }
    }

    /// Set async complete (for wait tasks)
    pub fn async_complete(mut self) -> Self {
        self.async_complete = true;
        self
    }

    /// Set start delay in seconds
    pub fn with_start_delay(mut self, seconds: i32) -> Self {
        self.start_delay = seconds;
        self
    }

    /// Set state change configuration for audit events
    pub fn with_state_change(mut self, config: StateChangeConfig) -> Self {
        self.on_state_change = Some(config);
        self
    }

    /// Set embedded task definition
    pub fn with_task_definition(mut self, task_def: EmbeddedTaskDef) -> Self {
        self.task_definition = Some(task_def);
        self
    }

    /// Create a join task with custom script
    pub fn join_with_script(
        task_ref_name: impl Into<String>,
        join_on: Vec<String>,
        script: impl Into<String>,
    ) -> Self {
        Self {
            name: "join".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Join,
            join_on,
            join_on_script: Some(script.into()),
            ..Default::default()
        }
    }

    /// Create an HTTP Poll task
    pub fn http_poll(task_ref_name: impl Into<String>, uri: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert("uri".to_string(), serde_json::json!(uri.into()));
        input.insert("method".to_string(), serde_json::json!("GET"));
        input.insert("pollingStrategy".to_string(), serde_json::json!("FIXED"));
        input.insert("pollingInterval".to_string(), serde_json::json!(1000));

        Self {
            name: "http_poll".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::HttpPoll,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set polling strategy (FIXED or EXPONENTIAL_BACKOFF)
    pub fn with_polling_strategy(mut self, strategy: impl Into<String>) -> Self {
        self.input_parameters.insert(
            "pollingStrategy".to_string(),
            serde_json::json!(strategy.into()),
        );
        self
    }

    /// Set polling interval in milliseconds
    pub fn with_polling_interval(mut self, interval_ms: i64) -> Self {
        self.input_parameters.insert(
            "pollingInterval".to_string(),
            serde_json::json!(interval_ms),
        );
        self
    }

    /// Set termination condition script for HTTP Poll
    pub fn with_termination_condition(mut self, script: impl Into<String>) -> Self {
        self.input_parameters.insert(
            "terminationCondition".to_string(),
            serde_json::json!(script.into()),
        );
        self
    }

    /// Create a wait for webhook task
    pub fn wait_for_webhook(task_ref_name: impl Into<String>) -> Self {
        Self {
            name: "wait_for_webhook".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::WaitForWebhook,
            ..Default::default()
        }
    }

    /// Set webhook matches configuration
    pub fn with_matches(mut self, matches: HashMap<String, String>) -> Self {
        let matches_value: HashMap<String, serde_json::Value> = matches
            .into_iter()
            .map(|(k, v)| (k, serde_json::json!(v)))
            .collect();
        self.input_parameters
            .insert("matches".to_string(), serde_json::json!(matches_value));
        self
    }

    /// Create an LLM text complete task
    pub fn llm_text_complete(
        task_ref_name: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        prompt_name: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "llmProvider".to_string(),
            serde_json::json!(llm_provider.into()),
        );
        input.insert("model".to_string(), serde_json::json!(model.into()));
        input.insert(
            "promptName".to_string(),
            serde_json::json!(prompt_name.into()),
        );

        Self {
            name: "llm_text_complete".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmTextComplete,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create an LLM chat complete task
    pub fn llm_chat_complete(
        task_ref_name: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "llmProvider".to_string(),
            serde_json::json!(llm_provider.into()),
        );
        input.insert("model".to_string(), serde_json::json!(model.into()));
        input.insert("messages".to_string(), serde_json::json!([]));

        Self {
            name: "llm_chat_complete".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmChatComplete,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set instructions template for LLM tasks
    pub fn with_instructions_template(mut self, template_name: impl Into<String>) -> Self {
        self.input_parameters.insert(
            "instructionsTemplate".to_string(),
            serde_json::json!(template_name.into()),
        );
        self
    }

    /// Set messages for chat complete
    pub fn with_messages(mut self, messages: Vec<ChatMessage>) -> Self {
        self.input_parameters.insert(
            "messages".to_string(),
            serde_json::to_value(messages).unwrap_or_default(),
        );
        self
    }

    /// Set temperature for LLM tasks
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.input_parameters
            .insert("temperature".to_string(), serde_json::json!(temp));
        self
    }

    /// Set max tokens for LLM tasks
    pub fn with_max_tokens(mut self, tokens: i32) -> Self {
        self.input_parameters
            .insert("maxTokens".to_string(), serde_json::json!(tokens));
        self
    }

    /// Set top_p for LLM tasks
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.input_parameters
            .insert("topP".to_string(), serde_json::json!(top_p));
        self
    }

    /// Set prompt variables for LLM tasks
    pub fn with_prompt_variables(mut self, variables: HashMap<String, serde_json::Value>) -> Self {
        self.input_parameters
            .insert("promptVariables".to_string(), serde_json::json!(variables));
        self
    }

    /// Add a single prompt variable
    pub fn with_prompt_variable(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        let vars = self
            .input_parameters
            .entry("promptVariables".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let serde_json::Value::Object(map) = vars {
            map.insert(key.into(), value.into());
        }
        self
    }

    /// Create an LLM generate embeddings task
    pub fn llm_generate_embeddings(
        task_ref_name: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "llmProvider".to_string(),
            serde_json::json!(llm_provider.into()),
        );
        input.insert("model".to_string(), serde_json::json!(model.into()));
        input.insert("text".to_string(), serde_json::json!(text.into()));

        Self {
            name: "llm_generate_embeddings".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmGenerateEmbeddings,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create an LLM search index task
    pub fn llm_search_index(
        task_ref_name: impl Into<String>,
        vector_db: impl Into<String>,
        index: impl Into<String>,
        query: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert("vectorDB".to_string(), serde_json::json!(vector_db.into()));
        input.insert("index".to_string(), serde_json::json!(index.into()));
        input.insert("query".to_string(), serde_json::json!(query.into()));

        Self {
            name: "llm_search_index".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmSearchIndex,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set namespace for vector DB tasks
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.input_parameters
            .insert("namespace".to_string(), serde_json::json!(namespace.into()));
        self
    }

    /// Set max results for search tasks
    pub fn with_max_results(mut self, max: i32) -> Self {
        self.input_parameters
            .insert("maxResults".to_string(), serde_json::json!(max));
        self
    }

    /// Set embedding model for vector DB tasks
    pub fn with_embedding_model(
        mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        self.input_parameters.insert(
            "embeddingModelProvider".to_string(),
            serde_json::json!(provider.into()),
        );
        self.input_parameters.insert(
            "embeddingModel".to_string(),
            serde_json::json!(model.into()),
        );
        self
    }

    /// Create an LLM index text task
    pub fn llm_index_text(
        task_ref_name: impl Into<String>,
        vector_db: impl Into<String>,
        index: impl Into<String>,
        text: impl Into<String>,
        doc_id: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert("vectorDB".to_string(), serde_json::json!(vector_db.into()));
        input.insert("index".to_string(), serde_json::json!(index.into()));
        input.insert("text".to_string(), serde_json::json!(text.into()));
        input.insert("docId".to_string(), serde_json::json!(doc_id.into()));

        Self {
            name: "llm_index_text".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmIndexText,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Create an LLM index document task
    pub fn llm_index_document(
        task_ref_name: impl Into<String>,
        vector_db: impl Into<String>,
        index: impl Into<String>,
        url: impl Into<String>,
        doc_id: impl Into<String>,
    ) -> Self {
        let mut input = HashMap::new();
        input.insert("vectorDB".to_string(), serde_json::json!(vector_db.into()));
        input.insert("index".to_string(), serde_json::json!(index.into()));
        input.insert("url".to_string(), serde_json::json!(url.into()));
        input.insert("docId".to_string(), serde_json::json!(doc_id.into()));

        Self {
            name: "llm_index_document".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::LlmIndexDocument,
            input_parameters: input,
            ..Default::default()
        }
    }

    /// Set media type for document tasks
    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.input_parameters.insert(
            "mediaType".to_string(),
            serde_json::json!(media_type.into()),
        );
        self
    }

    /// Set metadata for indexing tasks
    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.input_parameters
            .insert("metadata".to_string(), serde_json::json!(metadata));
        self
    }

    /// Create a dynamic task
    pub fn dynamic(task_ref_name: impl Into<String>, dynamic_task_name: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert(
            "dynamicTaskName".to_string(),
            serde_json::json!(dynamic_task_name.into()),
        );

        Self {
            name: "dynamic".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::Dynamic,
            input_parameters: input,
            dynamic_task_name_param: Some("dynamicTaskName".to_string()),
            ..Default::default()
        }
    }

    /// Create a get document task
    pub fn get_document(task_ref_name: impl Into<String>, url: impl Into<String>) -> Self {
        let mut input = HashMap::new();
        input.insert("url".to_string(), serde_json::json!(url.into()));

        Self {
            name: "get_document".to_string(),
            task_reference_name: task_ref_name.into(),
            task_type: TaskType::GetDocument,
            input_parameters: input,
            ..Default::default()
        }
    }
}

/// Chat message for LLM chat complete
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Role (user, assistant, system)
    pub role: String,

    /// Message content
    pub message: String,
}

impl ChatMessage {
    /// Create a user message
    pub fn user(message: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            message: message.into(),
        }
    }

    /// Create an assistant message
    pub fn assistant(message: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            message: message.into(),
        }
    }

    /// Create a system message
    pub fn system(message: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            message: message.into(),
        }
    }
}

/// Subworkflow parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubWorkflowParams {
    /// Subworkflow name
    pub name: String,

    /// Subworkflow version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,

    /// Task to domain mapping
    #[serde(default)]
    pub task_to_domain: HashMap<String, String>,
}

/// Workflow definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDef {
    /// Workflow name
    pub name: String,

    /// Workflow description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Workflow version
    #[serde(default = "default_version")]
    pub version: i32,

    /// Tasks in the workflow
    #[serde(default)]
    pub tasks: Vec<WorkflowTask>,

    /// Input parameters
    #[serde(default)]
    pub input_parameters: Vec<String>,

    /// Output parameters
    #[serde(default)]
    pub output_parameters: HashMap<String, serde_json::Value>,

    /// Failure workflow name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_workflow: Option<String>,

    /// Schema version
    #[serde(default = "default_schema_version")]
    pub schema_version: i32,

    /// Restartable flag
    #[serde(default = "default_true")]
    pub restartable: bool,

    /// Workflow status listener enabled
    #[serde(default)]
    pub workflow_status_listener_enabled: bool,

    /// Owner email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_email: Option<String>,

    /// Timeout policy
    #[serde(default)]
    pub timeout_policy: WorkflowTimeoutPolicy,

    /// Timeout in seconds
    #[serde(default)]
    pub timeout_seconds: i64,

    /// Variables
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,

    /// Input template
    #[serde(default)]
    pub input_template: HashMap<String, serde_json::Value>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Create time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
}

fn default_version() -> i32 {
    1
}

fn default_schema_version() -> i32 {
    2
}

fn default_true() -> bool {
    true
}

impl WorkflowDef {
    /// Create a new workflow definition
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: 1,
            schema_version: 2,
            restartable: true,
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set version
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = version;
        self
    }

    /// Add a task to the workflow
    pub fn with_task(mut self, task: WorkflowTask) -> Self {
        self.tasks.push(task);
        self
    }

    /// Set tasks
    pub fn with_tasks(mut self, tasks: Vec<WorkflowTask>) -> Self {
        self.tasks = tasks;
        self
    }

    /// Set input parameters
    pub fn with_input_parameters(mut self, params: Vec<String>) -> Self {
        self.input_parameters = params;
        self
    }

    /// Set output parameters
    pub fn with_output_parameters(mut self, params: HashMap<String, serde_json::Value>) -> Self {
        self.output_parameters = params;
        self
    }

    /// Add an output parameter
    pub fn with_output_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.output_parameters
            .insert(key.into(), serde_json::Value::String(value.into()));
        self
    }

    /// Set failure workflow
    pub fn with_failure_workflow(mut self, name: impl Into<String>) -> Self {
        self.failure_workflow = Some(name.into());
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, seconds: i64, policy: WorkflowTimeoutPolicy) -> Self {
        self.timeout_seconds = seconds;
        self.timeout_policy = policy;
        self
    }

    /// Set owner email
    pub fn with_owner(mut self, email: impl Into<String>) -> Self {
        self.owner_email = Some(email.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_def_builder() {
        let wf = WorkflowDef::new("my_workflow")
            .with_description("Test workflow")
            .with_version(1)
            .with_task(WorkflowTask::simple("task1", "task1_ref"))
            .with_task(WorkflowTask::simple("task2", "task2_ref"));

        assert_eq!(wf.name, "my_workflow");
        assert_eq!(wf.description, Some("Test workflow".to_string()));
        assert_eq!(wf.tasks.len(), 2);
    }

    #[test]
    fn test_workflow_task_builder() {
        let task = WorkflowTask::simple("my_task", "my_task_ref")
            .with_input_param("name", "value")
            .with_description("A simple task");

        assert_eq!(task.name, "my_task");
        assert_eq!(task.task_reference_name, "my_task_ref");
        assert_eq!(task.task_type, TaskType::Simple);
        assert!(task.input_parameters.contains_key("name"));
    }

    #[test]
    fn test_workflow_def_serialization() {
        let wf = WorkflowDef::new("test").with_task(WorkflowTask::simple("t1", "t1_ref"));

        let json = serde_json::to_string(&wf).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"tasks\":["));
    }
}
