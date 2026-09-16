// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::{ConductorError, Result};

use super::credentials::Credentials;
use super::def::AgentDef;
use super::guardrail::Guardrail;

/// Tool invocation mechanism.
///
/// Python has no formal enum for this — `tool_type` is a bare string on `ToolDef`. This crate
/// uses a closed enum instead (a deliberate Rust-native narrowing, not a python type it's
/// matching), so `as_str()`'s wire strings are what must match python's string literals exactly,
/// not the variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    Worker,
    Http,
    Mcp,
    AgentTool,
    Human,
    /// OpenAPI/Swagger/Postman-spec-driven tool discovery (python's `api_tool`). The server
    /// fetches and parses the spec at compile time and expands it into individual tools — this
    /// crate (like python) never parses the spec itself.
    Api,
    /// `Conductor GENERATE_IMAGE` system task (python's `image_tool`).
    GenerateImage,
    /// `Conductor GENERATE_AUDIO` system task (python's `audio_tool`).
    GenerateAudio,
    /// `Conductor GENERATE_VIDEO` system task (python's `video_tool`).
    GenerateVideo,
    /// `Conductor GENERATE_PDF` system task (python's `pdf_tool`).
    GeneratePdf,
    /// `Conductor LLM_INDEX_TEXT` system task — indexes documents into a vector DB (python's
    /// `index_tool`).
    RagIndex,
    /// `Conductor LLM_SEARCH_INDEX` system task — searches a vector DB (python's `search_tool`).
    RagSearch,
    /// `Conductor PULL_WORKFLOW_MESSAGES` system task — dequeues from the Workflow Message
    /// Queue (python's `wait_for_message_tool`).
    PullWorkflowMessages,
}

impl ToolType {
    /// Wire-format string, matching python-sdk's `tool_type=` string literals exactly.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolType::Worker => "worker",
            ToolType::Http => "http",
            ToolType::Mcp => "mcp",
            ToolType::AgentTool => "agent_tool",
            ToolType::Human => "human",
            ToolType::Api => "api",
            ToolType::GenerateImage => "generate_image",
            ToolType::GenerateAudio => "generate_audio",
            ToolType::GenerateVideo => "generate_video",
            ToolType::GeneratePdf => "generate_pdf",
            ToolType::RagIndex => "rag_index",
            ToolType::RagSearch => "rag_search",
            ToolType::PullWorkflowMessages => "pull_workflow_messages",
        }
    }
}

/// Session-scoped context available to a tool handler, matching python-sdk's `ToolContext`
/// dataclass (`tool.py`) field-for-field.
///
/// **Only `execution_id` and `state` carry real data.** Confirmed by reading
/// `runtime/_dispatch.py` directly: `agent_name`/`session_id`/`metadata`/`dependencies` are
/// populated from a module-level `_current_context = {}` dict that is read via `.get(key, "")`
/// in exactly one place and never *written* anywhere in python-sdk — dead ambient-context
/// scaffolding, not a real data path, in python today. `execution_id` comes from the polled
/// task's `workflow_instance_id`; `state` round-trips through the task wire format:
/// `_dispatch.py` reads a `_agent_state` input key into `ctx.state` before the call and, if the
/// handler leaves `state` non-empty afterward, folds it back into the task output as
/// `_state_updates` for the server to persist into the next call's `_agent_state`. This port
/// reproduces exactly that — the same 4 fields are always empty, and `state` is the only field
/// with a real read/write path — rather than inventing data for fields python itself never
/// populates.
///
/// `state` is `Arc<Mutex<...>>`, not a plain `HashMap`, so `ToolWorker` (`super::runtime`)-
/// equivalent dispatch code can inspect it *after* an async handler call returns and fold any
/// accumulated entries into the task output — the same mutate-in-place-then-read-back shape
/// python's single mutable `ctx.state` dict gets from being the same object before and after
/// the call, done here with an explicit, thread-safe handle instead of relying on shared
/// ambient state (matching this crate's existing, deliberate departure from python's
/// process-wide `contextvars` approach for credentials — see `docs/agents/secrets-and-credentials.md`).
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub session_id: String,
    pub execution_id: String,
    pub agent_name: String,
    pub metadata: HashMap<String, Value>,
    pub dependencies: HashMap<String, Value>,
    state: Arc<std::sync::Mutex<HashMap<String, Value>>>,
}

impl ToolContext {
    /// Read a value the current or a previous tool call in this agent run stored under `key`.
    #[must_use]
    pub fn get_state(&self, key: &str) -> Option<Value> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    /// Record a value for the server to persist into subsequent tool calls in this agent run.
    pub fn set_state(&self, key: impl Into<String>, value: Value) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.into(), value);
    }

    /// A snapshot of every key currently held, used by dispatch code to build the task output's
    /// `_state_updates` after a handler call returns.
    #[must_use]
    pub fn state_snapshot(&self) -> HashMap<String, Value> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Build a context pre-seeded with `state` — used by
    /// [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve)'s tool dispatch to inject
    /// the polled task's `_agent_state`, not intended for other callers.
    pub(super) fn with_initial_state(mut self, state: HashMap<String, Value>) -> Self {
        self.state = Arc::new(std::sync::Mutex::new(state));
        self
    }
}

/// Boxed async tool handler: takes the raw JSON arguments, the resolved [`Credentials`], and the
/// [`ToolContext`] for this call, returns raw JSON output.
///
/// Kept as raw `Value -> Value` (rather than generic over `T`) so `ToolDef` itself can stay
/// non-generic and be stored in a plain `Vec<ToolDef>` on [`AgentDef`]. [`ToolDef::function`],
/// [`ToolDef::function_with_credentials`], and [`ToolDef::function_with_context`] are the generic
/// entry points that wrap a strongly-typed `Fn(T) -> Fut` / `Fn(T, &Credentials) -> Fut` /
/// `Fn(T, ToolContext) -> Fut` into this shape — each wrapper simply ignores whichever of
/// `Credentials`/`ToolContext` its own signature doesn't take, so every constructor produces the
/// same `ToolHandler` shape and a caller invoking a tool never needs to know which constructor
/// built it.
pub type ToolHandler = Arc<
    dyn Fn(Value, Credentials, ToolContext) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>>
        + Send
        + Sync,
>;

/// Declarative tool definition attachable to an [`AgentDef`].
///
/// Constructed via one of the typed constructors ([`ToolDef::function`], [`ToolDef::http`],
/// [`ToolDef::mcp`], [`ToolDef::agent`], [`ToolDef::human`]) — each sets `tool_type` and
/// `config` consistently for its wire shape, so there is no bare public "generic" constructor.
#[derive(Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub tool_type: ToolType,
    pub approval_required: bool,
    pub stateful: bool,
    pub timeout_seconds: Option<u64>,
    pub max_calls: Option<u32>,
    pub config: HashMap<String, Value>,
    pub credentials: Vec<String>,
    pub sub_agent: Option<Box<AgentDef>>,
    pub handler: Option<ToolHandler>,
    /// Guardrails scoped to this tool, independent of the owning agent's
    /// [`AgentDef::guardrails`](super::def::AgentDef::guardrails) — matching python-sdk's
    /// `ToolDef.guardrails: List[Any]`. Serialized as `"guardrails"` on the tool's wire config
    /// (`config_serializer.py::_serialize_tool`); custom-function guardrails found here get a
    /// worker from [`AgentRuntime::serve`](super::runtime::AgentRuntime::serve) exactly like
    /// agent-level ones do, registered under the guardrail's own name.
    pub guardrails: Vec<Guardrail>,
}

impl std::fmt::Debug for ToolDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolDef")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("input_schema", &self.input_schema)
            .field("output_schema", &self.output_schema)
            .field("tool_type", &self.tool_type)
            .field("approval_required", &self.approval_required)
            .field("stateful", &self.stateful)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("max_calls", &self.max_calls)
            .field("config", &self.config)
            .field("credentials", &self.credentials)
            .field("sub_agent", &self.sub_agent)
            .field("handler", &self.handler.as_ref().map(|_| "Fn(..)"))
            .field("guardrails", &self.guardrails)
            .finish()
    }
}

impl ToolDef {
    fn base(name: impl Into<String>, description: impl Into<String>, tool_type: ToolType) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: Value::Null,
            output_schema: Value::Null,
            tool_type,
            approval_required: false,
            stateful: false,
            timeout_seconds: None,
            max_calls: None,
            config: HashMap::new(),
            credentials: Vec::new(),
            sub_agent: None,
            handler: None,
            guardrails: Vec::new(),
        }
    }

    /// A locally-invoked function tool. `input_schema` should describe `T`'s shape (typically
    /// generated via `crate::schema::generate_schema::<T>(true)`); the handler receives `T`
    /// deserialized from the raw JSON arguments.
    pub fn function<T, F, Fut>(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Self
    where
        T: DeserializeOwned,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let mut tool = Self::base(name, description, ToolType::Worker);
        tool.input_schema = input_schema;
        let handler = Arc::new(handler);
        tool.handler = Some(Arc::new(
            move |raw: Value, _credentials: Credentials, _context: ToolContext| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let args: T = serde_json::from_value(raw)?;
                    handler(args).await
                })
            },
        ));
        tool
    }

    /// A locally-invoked function tool whose handler also receives the resolved [`Credentials`]
    /// for this call — the entry point the `#[tool(credentials = [...])]` macro targets when a
    /// second `&Credentials` parameter is present (see `docs/agents/secrets-and-credentials.md`).
    /// `input_schema` still only describes `T`'s shape; `Credentials` is threaded in separately
    /// at call time, not part of the JSON arguments.
    pub fn function_with_credentials<T, F, Fut>(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Self
    where
        T: DeserializeOwned,
        F: Fn(T, &Credentials) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let mut tool = Self::base(name, description, ToolType::Worker);
        tool.input_schema = input_schema;
        let handler = Arc::new(handler);
        tool.handler = Some(Arc::new(
            move |raw: Value, credentials: Credentials, _context: ToolContext| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let args: T = serde_json::from_value(raw)?;
                    handler(args, &credentials).await
                })
            },
        ));
        tool
    }

    /// A locally-invoked function tool whose handler also receives the [`ToolContext`] for this
    /// call — the entry point for tools that declare a `context: ToolContext` parameter in
    /// python (matching `_dispatch.py`'s `_needs_context` check). `input_schema` still only
    /// describes `T`'s shape; the context is threaded in separately at call time, not part of
    /// the JSON arguments.
    pub fn function_with_context<T, F, Fut>(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        handler: F,
    ) -> Self
    where
        T: DeserializeOwned,
        F: Fn(T, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let mut tool = Self::base(name, description, ToolType::Worker);
        tool.input_schema = input_schema;
        let handler = Arc::new(handler);
        tool.handler = Some(Arc::new(
            move |raw: Value, _credentials: Credentials, context: ToolContext| {
                let handler = Arc::clone(&handler);
                Box::pin(async move {
                    let args: T = serde_json::from_value(raw)?;
                    handler(args, context).await
                })
            },
        ));
        tool
    }

    /// An HTTP-backed tool. `url` and `headers` values may reference declared credentials via
    /// `${NAME}` placeholders; any placeholder not present in `credentials` is rejected.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `url` or any `headers` value references a `${NAME}` placeholder not present in `credentials`.
    pub fn http(
        name: impl Into<String>,
        description: impl Into<String>,
        url: impl Into<String>,
        method: &str,
        headers: HashMap<String, String>,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let url = url.into();
        validate_credential_placeholders(&headers, &credentials)?;
        validate_credential_placeholders(
            &HashMap::from([("url".to_owned(), url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Http);
        tool.credentials = credentials;
        // Matches python's `http_tool`: `input_schema=input_schema or {"type": "object",
        // "properties": {}}` -- a tool with no input parameters still needs a non-null schema
        // on the wire (a bare `{"type": "object"}`, or `null`, is a different JSON value from
        // `{"type": "object", "properties": {}}` and fails the mock-LLM-provider's exact
        // request match on any tool-calling turn).
        tool.input_schema = serde_json::json!({"type": "object", "properties": {}});
        tool.config.insert("url".to_owned(), Value::String(url));
        tool.config
            .insert("method".to_owned(), Value::String(method.to_uppercase()));
        tool.config
            .insert("headers".to_owned(), serde_json::to_value(&headers)?);
        tool.config.insert(
            "accept".to_owned(),
            Value::Array(vec![Value::String("application/json".to_owned())]),
        );
        tool.config.insert(
            "contentType".to_owned(),
            Value::String("application/json".to_owned()),
        );
        Ok(tool)
    }

    /// An HTTP-backed tool with URI templating: `path_template`'s `{param}` placeholders are
    /// filled in from the LLM's call arguments (URL-encoded) and appended to `url`; `query_params`
    /// names which arguments get appended to the query string instead of the request body. Both
    /// are real, general `HttpTaskConfig` wire fields the compiled `EnrichTools` script consumes
    /// at dispatch time — not specific to any one caller — but [`ToolDef::http`] doesn't expose
    /// them because no python factory does either; `ocg.py` (this crate's `super::ocg`) hand-
    /// builds a `ToolDef` with these fields directly rather than going through `http_tool()`.
    /// Unlike [`ToolDef::http`], no `accept`/`contentType` defaults are set, matching that
    /// hand-built shape exactly. Same `${NAME}` credential-placeholder validation as
    /// [`ToolDef::http`].
    #[expect(clippy::too_many_arguments)]
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `url` or any `headers` value references a `${NAME}` placeholder not present in `credentials`.
    pub fn http_templated(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        url: impl Into<String>,
        method: &str,
        path_template: Option<String>,
        query_params: Option<Vec<String>>,
        headers: HashMap<String, String>,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let url = url.into();
        validate_credential_placeholders(&headers, &credentials)?;
        validate_credential_placeholders(
            &HashMap::from([("url".to_owned(), url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Http);
        tool.input_schema = input_schema;
        tool.credentials = credentials;
        tool.config.insert("url".to_owned(), Value::String(url));
        tool.config
            .insert("method".to_owned(), Value::String(method.to_uppercase()));
        if let Some(path_template) = path_template {
            tool.config
                .insert("pathTemplate".to_owned(), Value::String(path_template));
        }
        if let Some(query_params) = query_params {
            tool.config.insert(
                "queryParams".to_owned(),
                Value::Array(query_params.into_iter().map(Value::String).collect()),
            );
        }
        if !headers.is_empty() {
            tool.config
                .insert("headers".to_owned(), serde_json::to_value(&headers)?);
        }
        Ok(tool)
    }

    /// An MCP-backed tool. Same `${NAME}` credential-placeholder validation as [`ToolDef::http`].
    ///
    /// `max_tools` is the threshold (matching python's `mcp_tool`'s `max_tools: int = 64`
    /// default) above which the server compiles a runtime LLM-filtering step instead of listing
    /// every discovered MCP tool directly — always emitted on the wire as `config["max_tools"]`
    /// (python does this unconditionally too), since the server's own compiler falls back to a
    /// *different*, lower default (32) when the key is absent entirely, silently changing
    /// compiled behavior for any MCP server exposing more than 32 tools. `tool_names` is an
    /// optional whitelist of MCP tool names to include, only emitted when `Some`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `server_url` or any `headers` value references a `${NAME}` placeholder not present in `credentials`.
    pub fn mcp(
        server_url: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        headers: HashMap<String, String>,
        tool_names: Option<Vec<String>>,
        max_tools: u32,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let server_url = server_url.into();
        validate_credential_placeholders(&headers, &credentials)?;
        validate_credential_placeholders(
            &HashMap::from([("server_url".to_owned(), server_url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Mcp);
        tool.credentials = credentials;
        tool.config
            .insert("server_url".to_owned(), Value::String(server_url));
        tool.config
            .insert("headers".to_owned(), serde_json::to_value(&headers)?);
        if let Some(tool_names) = tool_names {
            tool.config.insert(
                "tool_names".to_owned(),
                Value::Array(tool_names.into_iter().map(Value::String).collect()),
            );
        }
        tool.config
            .insert("max_tools".to_owned(), Value::from(max_tools));
        Ok(tool)
    }

    /// A tool that delegates to a sub-agent (`toolType: "agent_tool"`), recursively serialized
    /// into `config.agentConfig` by [`super::AgentConfigSerializer`].
    #[must_use]
    pub fn agent(agent: AgentDef) -> Self {
        let mut tool = Self::base(
            agent.name.clone(),
            format!("Delegate to agent '{}'", agent.name),
            ToolType::AgentTool,
        );
        tool.input_schema = serde_json::json!({
            "type": "object",
            "properties": { "request": { "type": "string" } },
            "required": ["request"]
        });
        tool.sub_agent = Some(Box::new(agent));
        tool
    }

    /// A tool that pauses agent execution to ask a human a question.
    pub fn human(name: impl Into<String>, description: impl Into<String>) -> Self {
        let mut tool = Self::base(name, description, ToolType::Human);
        tool.input_schema = serde_json::json!({
            "type": "object",
            "properties": { "question": { "type": "string" } },
            "required": ["question"]
        });
        tool
    }

    /// A tool built from an `OpenAPI` spec, Swagger spec, Postman collection, or base URL (python's
    /// `api_tool`). At compile time the *server* fetches and parses `url`, auto-detecting the
    /// format, and expands it into individual tools — this crate never parses the spec itself,
    /// matching python exactly. Tool calls execute as ordinary Conductor `HTTP` tasks; no worker
    /// process is needed.
    ///
    /// `url`/`headers` values may reference declared credentials via `${NAME}` placeholders —
    /// same validation as [`ToolDef::http`]/[`ToolDef::mcp`]. `tool_names` is an optional
    /// whitelist of operation IDs to include; `max_tools` (matches python's default of `64`)
    /// is the threshold above which the server uses a filter LLM to select the most relevant
    /// operations for the user's prompt at run time.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Agent`] if `url` or any `headers` value references a `${NAME}` placeholder not present in `credentials`.
    pub fn api(
        url: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        headers: HashMap<String, String>,
        tool_names: Option<Vec<String>>,
        max_tools: u32,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let url = url.into();
        validate_credential_placeholders(&headers, &credentials)?;
        validate_credential_placeholders(
            &HashMap::from([("url".to_owned(), url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Api);
        tool.credentials = credentials;
        tool.config.insert("url".to_owned(), Value::String(url));
        if !headers.is_empty() {
            tool.config
                .insert("headers".to_owned(), serde_json::to_value(&headers)?);
        }
        if let Some(tool_names) = tool_names {
            tool.config.insert(
                "tool_names".to_owned(),
                Value::Array(tool_names.into_iter().map(Value::String).collect()),
            );
        }
        tool.config
            .insert("max_tools".to_owned(), Value::from(max_tools));
        Ok(tool)
    }

    /// A tool that generates images via the Conductor `GENERATE_IMAGE` system task (python's
    /// `image_tool`). No worker process is needed — the server calls the AI provider directly.
    /// `input_schema` defaults to a schema with `prompt`/`style`/`width`/`height`/`size`/`n`/
    /// `outputFormat`/`weight` (matching python's default exactly) when `None`. `defaults` are
    /// extra static parameters baked into the task config (python's `**defaults`).
    pub fn image(
        name: impl Into<String>,
        description: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        input_schema: Option<Value>,
        defaults: HashMap<String, Value>,
    ) -> Self {
        Self::media_tool(
            ToolType::GenerateImage,
            "GENERATE_IMAGE",
            name,
            description,
            llm_provider,
            model,
            input_schema.unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string", "description": "Text description of the image to generate."},
                        "style": {"type": "string", "description": "Image style: 'vivid' or 'natural'."},
                        "width": {"type": "integer", "description": "Image width in pixels.", "default": 1024},
                        "height": {"type": "integer", "description": "Image height in pixels.", "default": 1024},
                        "size": {"type": "string", "description": "Image size (e.g. '1024x1024'). Alternative to width/height."},
                        "n": {"type": "integer", "description": "Number of images to generate.", "default": 1},
                        "outputFormat": {"type": "string", "description": "Output format: 'png', 'jpg', or 'webp'.", "default": "png"},
                        "weight": {"type": "number", "description": "Image weight parameter."},
                    },
                    "required": ["prompt"],
                })
            }),
            defaults,
        )
    }

    /// A tool that generates audio / text-to-speech via the Conductor `GENERATE_AUDIO` system
    /// task (python's `audio_tool`). `input_schema` defaults to a schema with `text`/`voice`/
    /// `speed`/`responseFormat`/`n` when `None`.
    pub fn audio(
        name: impl Into<String>,
        description: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        input_schema: Option<Value>,
        defaults: HashMap<String, Value>,
    ) -> Self {
        Self::media_tool(
            ToolType::GenerateAudio,
            "GENERATE_AUDIO",
            name,
            description,
            llm_provider,
            model,
            input_schema.unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string", "description": "Text to convert to speech."},
                        "voice": {
                            "type": "string",
                            "description": "Voice to use.",
                            "enum": ["alloy", "echo", "fable", "onyx", "nova", "shimmer"],
                            "default": "alloy",
                        },
                        "speed": {"type": "number", "description": "Speech speed multiplier (0.25 to 4.0).", "default": 1.0},
                        "responseFormat": {"type": "string", "description": "Audio format: 'mp3', 'wav', 'opus', 'aac', or 'flac'.", "default": "mp3"},
                        "n": {"type": "integer", "description": "Number of audio outputs to generate.", "default": 1},
                    },
                    "required": ["text"],
                })
            }),
            defaults,
        )
    }

    /// A tool that generates video via the Conductor `GENERATE_VIDEO` system task (python's
    /// `video_tool`). Video generation is typically async — the server submits the job and
    /// polls until ready. `input_schema` defaults to a schema with `prompt`/`duration`/`style`
    /// (plus many optional provider-specific fields, matching python's default exactly) when
    /// `None`.
    pub fn video(
        name: impl Into<String>,
        description: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        input_schema: Option<Value>,
        defaults: HashMap<String, Value>,
    ) -> Self {
        Self::media_tool(
            ToolType::GenerateVideo,
            "GENERATE_VIDEO",
            name,
            description,
            llm_provider,
            model,
            input_schema.unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string", "description": "Text description of the video scene."},
                        "inputImage": {"type": "string", "description": "Base64-encoded or URL image for image-to-video generation."},
                        "duration": {"type": "integer", "description": "Video duration in seconds.", "default": 5},
                        "width": {"type": "integer", "description": "Video width in pixels.", "default": 1280},
                        "height": {"type": "integer", "description": "Video height in pixels.", "default": 720},
                        "fps": {"type": "integer", "description": "Frames per second.", "default": 24},
                        "outputFormat": {"type": "string", "description": "Video format (e.g. 'mp4').", "default": "mp4"},
                        "style": {"type": "string", "description": "Video style (e.g. 'cinematic', 'natural')."},
                        "motion": {"type": "string", "description": "Movement intensity (e.g. 'slow', 'normal', 'extreme')."},
                        "seed": {"type": "integer", "description": "Seed for reproducibility."},
                        "guidanceScale": {"type": "number", "description": "Prompt adherence strength (1.0 to 20.0)."},
                        "aspectRatio": {"type": "string", "description": "Aspect ratio (e.g. '16:9', '1:1')."},
                        "negativePrompt": {"type": "string", "description": "Description of what to exclude from the video."},
                        "personGeneration": {"type": "string", "description": "Controls for human figure generation."},
                        "resolution": {"type": "string", "description": "Quality level (e.g. '720p', '1080p')."},
                        "generateAudio": {"type": "boolean", "description": "Whether to generate audio with the video."},
                        "size": {"type": "string", "description": "Video size specification (e.g. '1280x720')."},
                        "n": {"type": "integer", "description": "Number of videos to generate.", "default": 1},
                        "maxDurationSeconds": {"type": "integer", "description": "Maximum duration ceiling in seconds."},
                        "maxCostDollars": {"type": "number", "description": "Maximum cost limit in dollars."},
                    },
                    "required": ["prompt"],
                })
            }),
            defaults,
        )
    }

    /// A tool that generates a PDF from markdown via the Conductor `GENERATE_PDF` system task
    /// (python's `pdf_tool`). No AI provider is needed — the server converts markdown to PDF
    /// directly. `input_schema` defaults to a schema with `markdown`/`pageSize`/`theme`/
    /// `baseFontSize` when `None`.
    ///
    /// Unlike python (which defaults `name`/`description` to `"generate_pdf"`/`"Generate a PDF
    /// document from markdown text."` since it's the only media tool with no required provider/
    /// model), this crate requires both explicitly — matching every other tool constructor's
    /// convention rather than special-casing this one. Pass those exact strings to reproduce
    /// python's defaults.
    pub fn pdf(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Option<Value>,
        defaults: HashMap<String, Value>,
    ) -> Self {
        let mut tool = Self::base(name, description, ToolType::GeneratePdf);
        tool.input_schema = input_schema.unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "markdown": {"type": "string", "description": "Markdown text to convert to PDF."},
                    "pageSize": {"type": "string", "description": "Page size: A4, LETTER, LEGAL, A3, or A5.", "default": "A4"},
                    "theme": {"type": "string", "description": "Style preset: 'default' or 'compact'.", "default": "default"},
                    "baseFontSize": {"type": "number", "description": "Base font size in points.", "default": 11},
                },
                "required": ["markdown"],
            })
        });
        let mut config = HashMap::from([(
            "taskType".to_owned(),
            Value::String("GENERATE_PDF".to_owned()),
        )]);
        config.extend(defaults);
        tool.config = config;
        tool
    }

    /// Internal helper shared by [`ToolDef::image`]/[`ToolDef::audio`]/[`ToolDef::video`] —
    /// mirrors python's `_media_tool`.
    #[expect(clippy::too_many_arguments)]
    fn media_tool(
        tool_type: ToolType,
        task_type: &str,
        name: impl Into<String>,
        description: impl Into<String>,
        llm_provider: impl Into<String>,
        model: impl Into<String>,
        input_schema: Value,
        defaults: HashMap<String, Value>,
    ) -> Self {
        let mut tool = Self::base(name, description, tool_type);
        tool.input_schema = input_schema;
        let mut config = HashMap::from([
            ("taskType".to_owned(), Value::String(task_type.to_owned())),
            ("llmProvider".to_owned(), Value::String(llm_provider.into())),
            ("model".to_owned(), Value::String(model.into())),
        ]);
        config.extend(defaults);
        tool.config = config;
        tool
    }

    /// A tool that indexes documents into a vector database via the Conductor `LLM_INDEX_TEXT`
    /// system task (python's `index_tool`). No worker process is needed. `namespace` defaults
    /// to `"default_ns"` when `None`, matching python. `input_schema` defaults to a schema with
    /// `text`/`docId`/`metadata` when `None`.
    #[expect(clippy::too_many_arguments)]
    pub fn rag_index(
        name: impl Into<String>,
        description: impl Into<String>,
        vector_db: impl Into<String>,
        index: impl Into<String>,
        embedding_model_provider: impl Into<String>,
        embedding_model: impl Into<String>,
        namespace: Option<String>,
        chunk_size: Option<u32>,
        chunk_overlap: Option<u32>,
        dimensions: Option<u32>,
        input_schema: Option<Value>,
    ) -> Self {
        let mut tool = Self::base(name, description, ToolType::RagIndex);
        tool.input_schema = input_schema.unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "The text content to index."},
                    "docId": {"type": "string", "description": "Unique document identifier."},
                    "metadata": {"type": "object", "description": "Optional metadata to store with the document."},
                },
                "required": ["text", "docId"],
            })
        });
        tool.config.insert(
            "taskType".to_owned(),
            Value::String("LLM_INDEX_TEXT".to_owned()),
        );
        tool.config
            .insert("vectorDB".to_owned(), Value::String(vector_db.into()));
        tool.config.insert(
            "namespace".to_owned(),
            Value::String(namespace.unwrap_or_else(|| "default_ns".to_owned())),
        );
        tool.config
            .insert("index".to_owned(), Value::String(index.into()));
        tool.config.insert(
            "embeddingModelProvider".to_owned(),
            Value::String(embedding_model_provider.into()),
        );
        tool.config.insert(
            "embeddingModel".to_owned(),
            Value::String(embedding_model.into()),
        );
        if let Some(chunk_size) = chunk_size {
            tool.config
                .insert("chunkSize".to_owned(), Value::from(chunk_size));
        }
        if let Some(chunk_overlap) = chunk_overlap {
            tool.config
                .insert("chunkOverlap".to_owned(), Value::from(chunk_overlap));
        }
        if let Some(dimensions) = dimensions {
            tool.config
                .insert("dimensions".to_owned(), Value::from(dimensions));
        }
        tool
    }

    /// A tool that searches a vector database via the Conductor `LLM_SEARCH_INDEX` system task
    /// (python's `search_tool`). No worker process is needed. `namespace` defaults to
    /// `"default_ns"`, `max_results` to `5` (both matching python) when `None`. `input_schema`
    /// defaults to a schema with just `query` when `None`.
    #[expect(clippy::too_many_arguments)]
    pub fn rag_search(
        name: impl Into<String>,
        description: impl Into<String>,
        vector_db: impl Into<String>,
        index: impl Into<String>,
        embedding_model_provider: impl Into<String>,
        embedding_model: impl Into<String>,
        namespace: Option<String>,
        max_results: Option<u32>,
        dimensions: Option<u32>,
        input_schema: Option<Value>,
    ) -> Self {
        let mut tool = Self::base(name, description, ToolType::RagSearch);
        tool.input_schema = input_schema.unwrap_or_else(|| {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "The search query."},
                },
                "required": ["query"],
            })
        });
        tool.config.insert(
            "taskType".to_owned(),
            Value::String("LLM_SEARCH_INDEX".to_owned()),
        );
        tool.config
            .insert("vectorDB".to_owned(), Value::String(vector_db.into()));
        tool.config.insert(
            "namespace".to_owned(),
            Value::String(namespace.unwrap_or_else(|| "default_ns".to_owned())),
        );
        tool.config
            .insert("index".to_owned(), Value::String(index.into()));
        tool.config.insert(
            "embeddingModelProvider".to_owned(),
            Value::String(embedding_model_provider.into()),
        );
        tool.config.insert(
            "embeddingModel".to_owned(),
            Value::String(embedding_model.into()),
        );
        tool.config.insert(
            "maxResults".to_owned(),
            Value::from(max_results.unwrap_or(5)),
        );
        if let Some(dimensions) = dimensions {
            tool.config
                .insert("dimensions".to_owned(), Value::from(dimensions));
        }
        tool
    }

    /// A tool that dequeues messages from the Workflow Message Queue via the Conductor
    /// `PULL_WORKFLOW_MESSAGES` system task (python's `wait_for_message_tool`). No worker
    /// process is needed. In blocking mode (`blocking = true`, the default python uses), the
    /// task stays `IN_PROGRESS` while the queue is empty; in non-blocking mode it completes
    /// immediately with whatever messages (if any) are already queued.
    pub fn wait_for_message(
        name: impl Into<String>,
        description: impl Into<String>,
        batch_size: u32,
        blocking: bool,
    ) -> Self {
        let mut tool = Self::base(name, description, ToolType::PullWorkflowMessages);
        tool.input_schema = serde_json::json!({"type": "object", "properties": {}});
        tool.config
            .insert("batchSize".to_owned(), Value::from(batch_size));
        if !blocking {
            tool.config
                .insert("blocking".to_owned(), Value::Bool(false));
        }
        tool
    }

    #[must_use]
    pub fn with_output_schema(mut self, schema: Value) -> Self {
        self.output_schema = schema;
        self
    }

    #[must_use]
    pub fn with_approval_required(mut self, approval_required: bool) -> Self {
        self.approval_required = approval_required;
        self
    }

    #[must_use]
    pub fn with_stateful(mut self, stateful: bool) -> Self {
        self.stateful = stateful;
        self
    }

    #[must_use]
    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    #[must_use]
    pub fn with_max_calls(mut self, max_calls: u32) -> Self {
        self.max_calls = Some(max_calls);
        self
    }

    /// Declare the credential names this tool needs. These names flow end-to-end: registration
    /// stamps them onto `TaskDef.runtime_metadata`, the server resolves and delivers values back
    /// on the polled `Task`, and a handler built via [`ToolDef::function_with_credentials`] reads
    /// them out of the `&Credentials` it's called with (see
    /// `docs/agents/secrets-and-credentials.md`). Also required (not merely declared) by
    /// [`ToolDef::http`] / [`ToolDef::mcp`]'s `${NAME}` placeholder validation.
    #[must_use]
    pub fn with_credentials(mut self, credentials: Vec<String>) -> Self {
        self.credentials = credentials;
        self
    }

    /// Add a guardrail scoped to this tool, independent of the owning agent's guardrails.
    /// Accumulates — call once per guardrail, matching python's `ToolDef(guardrails=[...])`.
    #[must_use]
    pub fn with_guardrail(mut self, guardrail: Guardrail) -> Self {
        self.guardrails.push(guardrail);
        self
    }

    #[must_use]
    pub fn with_guardrails(mut self, guardrails: impl IntoIterator<Item = Guardrail>) -> Self {
        self.guardrails.extend(guardrails);
        self
    }
}

fn validate_credential_placeholders(
    headers: &HashMap<String, String>,
    credentials: &[String],
) -> Result<()> {
    for value in headers.values() {
        for placeholder in extract_placeholders(value) {
            if !credentials.iter().any(|c| c == placeholder) {
                return Err(ConductorError::agent(format!(
                    "undeclared credential placeholder '${{{placeholder}}}': add '{placeholder}' \
                     to `credentials` or remove the placeholder"
                )));
            }
        }
    }
    Ok(())
}

/// Extracts the names inside `${NAME}` placeholders from a string, in order of appearance.
fn extract_placeholders(value: &str) -> Vec<&str> {
    let mut placeholders = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find('}') {
            placeholders.push(&after_open[..end]);
            rest = &after_open[end + 1..];
        } else {
            break;
        }
    }
    placeholders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Args {
        n: i32,
    }

    #[tokio::test]
    async fn test_function_tool_roundtrip() {
        let tool = ToolDef::function::<Args, _, _>(
            "double",
            "doubles a number",
            serde_json::json!({"type": "object"}),
            |args: Args| async move { Ok(Value::from(args.n * 2)) },
        );
        let handler = tool.handler.clone().unwrap();
        let result = handler(
            serde_json::json!({"n": 21}),
            Credentials::default(),
            ToolContext::default(),
        )
        .await
        .unwrap();
        assert_eq!(result, Value::from(42));
    }

    #[tokio::test]
    async fn test_function_with_credentials_tool_reads_credential() {
        let tool = ToolDef::function_with_credentials::<Args, _, _>(
            "double_with_token",
            "doubles a number, using a token",
            serde_json::json!({"type": "object"}),
            |args: Args, creds: &Credentials| {
                let token = creds.get("API_KEY").unwrap().to_owned();
                async move { Ok(Value::from(format!("{token}:{}", args.n * 2))) }
            },
        )
        .with_credentials(vec!["API_KEY".to_owned()]);
        assert_eq!(tool.credentials, vec!["API_KEY".to_owned()]);

        let mut values = HashMap::new();
        values.insert("API_KEY".to_owned(), "secret".to_owned());
        let creds = Credentials::new(values);

        let handler = tool.handler.clone().unwrap();
        let result = handler(serde_json::json!({"n": 21}), creds, ToolContext::default())
            .await
            .unwrap();
        assert_eq!(result, Value::from("secret:42"));
    }

    #[test]
    fn test_tool_context_set_and_get_state() {
        let ctx = ToolContext::default();
        assert_eq!(ctx.get_state("repo"), None);
        ctx.set_state("repo", Value::String("conductor".to_owned()));
        assert_eq!(
            ctx.get_state("repo"),
            Some(Value::String("conductor".to_owned()))
        );
    }

    #[test]
    fn test_tool_context_state_snapshot_reflects_mutations() {
        let ctx = ToolContext::default();
        ctx.set_state("a", Value::from(1));
        ctx.set_state("b", Value::from(2));
        let snapshot = ctx.state_snapshot();
        assert_eq!(snapshot.get("a"), Some(&Value::from(1)));
        assert_eq!(snapshot.get("b"), Some(&Value::from(2)));
    }

    #[test]
    fn test_tool_context_clone_shares_the_same_state() {
        let ctx = ToolContext::default();
        let cloned = ctx.clone();
        ctx.set_state("shared", Value::from(true));
        // Cloning a ToolContext clones the Arc, not the underlying state — a handler's clone
        // and the dispatcher's original see the same mutations, matching python's single
        // mutable `ctx.state` dict being the same object throughout one call.
        assert_eq!(cloned.get_state("shared"), Some(Value::from(true)));
    }

    #[test]
    fn test_tool_context_with_initial_state_seeds_snapshot() {
        let mut seed = HashMap::new();
        seed.insert("repo".to_owned(), Value::String("conductor".to_owned()));
        let ctx = ToolContext::default().with_initial_state(seed);
        assert_eq!(
            ctx.get_state("repo"),
            Some(Value::String("conductor".to_owned()))
        );
    }

    #[tokio::test]
    async fn test_function_with_context_tool_reads_and_writes_state() {
        #[derive(serde::Deserialize)]
        struct Args {
            n: i32,
        }

        let tool = ToolDef::function_with_context::<Args, _, _>(
            "increment",
            "increments a running total",
            serde_json::json!({"type": "object"}),
            |args: Args, ctx: ToolContext| async move {
                let previous = ctx.get_state("total").and_then(|v| v.as_i64()).unwrap_or(0);
                let total = previous + i64::from(args.n);
                ctx.set_state("total", Value::from(total));
                Ok(Value::from(total))
            },
        );

        let handler = tool.handler.clone().unwrap();
        let ctx = ToolContext::default();
        let result = handler(
            serde_json::json!({"n": 5}),
            Credentials::default(),
            ctx.clone(),
        )
        .await
        .unwrap();
        assert_eq!(result, Value::from(5));
        assert_eq!(ctx.get_state("total"), Some(Value::from(5)));
    }

    #[test]
    fn test_http_tool_rejects_undeclared_placeholder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer ${API_KEY}".to_owned());
        let result = ToolDef::http("t", "d", "https://example.com", "get", headers, vec![]);
        result.unwrap_err();
    }

    #[test]
    fn test_http_tool_accepts_declared_placeholder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer ${API_KEY}".to_owned());
        let result = ToolDef::http(
            "t",
            "d",
            "https://example.com",
            "get",
            headers,
            vec!["API_KEY".to_owned()],
        );
        assert!(result.is_ok());
        let tool = result.unwrap();
        assert_eq!(
            tool.config.get("method"),
            Some(&Value::String("GET".to_owned()))
        );
    }

    #[test]
    fn test_http_tool_defaults_to_empty_object_input_schema() {
        // Matches python's `http_tool`'s `input_schema or {"type": "object", "properties":
        // {}}` default -- not `null`, and not a bare `{"type": "object"}` with no `properties`
        // key, both of which are different JSON values on the wire.
        let tool = ToolDef::http(
            "t",
            "d",
            "https://example.com",
            "get",
            HashMap::new(),
            vec![],
        )
        .unwrap();
        assert_eq!(
            tool.input_schema,
            serde_json::json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn test_mcp_tool_always_emits_max_tools_defaulting_to_64() {
        // The server's own compiler falls back to a *different*, lower default (32) when
        // `max_tools` is absent from the wire config entirely -- matches python's `mcp_tool`,
        // which unconditionally sets `config["max_tools"] = max_tools` (default 64).
        let tool = ToolDef::mcp(
            "https://mcp.example.com",
            "t",
            "d",
            HashMap::new(),
            None,
            64,
            vec![],
        )
        .unwrap();
        assert_eq!(tool.config.get("max_tools"), Some(&Value::from(64)));
        assert!(!tool.config.contains_key("tool_names"));
    }

    #[test]
    fn test_mcp_tool_emits_tool_names_when_given() {
        let tool = ToolDef::mcp(
            "https://mcp.example.com",
            "t",
            "d",
            HashMap::new(),
            Some(vec!["search".to_owned(), "fetch".to_owned()]),
            32,
            vec![],
        )
        .unwrap();
        assert_eq!(
            tool.config.get("tool_names"),
            Some(&Value::from(vec!["search", "fetch"]))
        );
        assert_eq!(tool.config.get("max_tools"), Some(&Value::from(32)));
    }

    #[test]
    fn test_agent_tool_holds_sub_agent() {
        let sub = AgentDef::new("researcher").unwrap();
        let tool = ToolDef::agent(sub);
        assert_eq!(tool.tool_type, ToolType::AgentTool);
        assert!(tool.sub_agent.is_some());
    }

    #[test]
    fn test_api_tool_wire_shape() {
        let mut headers = HashMap::new();
        headers.insert(
            "Authorization".to_owned(),
            "Bearer ${STRIPE_KEY}".to_owned(),
        );
        let tool = ToolDef::api(
            "https://api.stripe.com/openapi.json",
            "stripe",
            "Stripe API tools",
            headers,
            None,
            20,
            vec!["STRIPE_KEY".to_owned()],
        )
        .unwrap();

        assert_eq!(tool.tool_type, ToolType::Api);
        assert_eq!(
            tool.config.get("url"),
            Some(&Value::String(
                "https://api.stripe.com/openapi.json".to_owned()
            ))
        );
        assert_eq!(tool.config.get("max_tools"), Some(&Value::from(20_u32)));
        assert!(!tool.config.contains_key("tool_names"));
        assert_eq!(tool.credentials, vec!["STRIPE_KEY".to_owned()]);
    }

    #[test]
    fn test_api_tool_rejects_undeclared_placeholder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_owned(), "Bearer ${MISSING}".to_owned());
        let result = ToolDef::api(
            "https://api.example.com/openapi.json",
            "t",
            "d",
            headers,
            None,
            64,
            vec![],
        );
        result.unwrap_err();
    }

    #[test]
    fn test_api_tool_includes_tool_names_when_given() {
        let tool = ToolDef::api(
            "https://api.example.com/openapi.json",
            "t",
            "d",
            HashMap::new(),
            Some(vec!["listUsers".to_owned(), "getUser".to_owned()]),
            64,
            vec![],
        )
        .unwrap();
        assert_eq!(
            tool.config.get("tool_names"),
            Some(&serde_json::json!(["listUsers", "getUser"]))
        );
    }

    #[test]
    fn test_image_tool_default_schema_and_config() {
        let tool = ToolDef::image("gen_image", "d", "openai", "dall-e-3", None, HashMap::new());
        assert_eq!(tool.tool_type, ToolType::GenerateImage);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("GENERATE_IMAGE".to_owned()))
        );
        assert_eq!(
            tool.config.get("llmProvider"),
            Some(&Value::String("openai".to_owned()))
        );
        assert_eq!(
            tool.config.get("model"),
            Some(&Value::String("dall-e-3".to_owned()))
        );
        assert!(tool.input_schema["properties"]["prompt"].is_object());
        assert_eq!(tool.input_schema["required"], serde_json::json!(["prompt"]));
    }

    #[test]
    fn test_image_tool_merges_extra_defaults() {
        let mut defaults = HashMap::new();
        defaults.insert("n".to_owned(), Value::from(2));
        let tool = ToolDef::image("t", "d", "openai", "dall-e-3", None, defaults);
        assert_eq!(tool.config.get("n"), Some(&Value::from(2)));
    }

    #[test]
    fn test_image_tool_custom_input_schema_overrides_default() {
        let custom = serde_json::json!({"type": "object", "properties": {}});
        let tool = ToolDef::image(
            "t",
            "d",
            "openai",
            "dall-e-3",
            Some(custom.clone()),
            HashMap::new(),
        );
        assert_eq!(tool.input_schema, custom);
    }

    #[test]
    fn test_audio_tool_default_schema_and_config() {
        let tool = ToolDef::audio("tts", "d", "openai", "tts-1", None, HashMap::new());
        assert_eq!(tool.tool_type, ToolType::GenerateAudio);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("GENERATE_AUDIO".to_owned()))
        );
        assert!(tool.input_schema["properties"]["voice"].is_object());
        assert_eq!(tool.input_schema["required"], serde_json::json!(["text"]));
    }

    #[test]
    fn test_video_tool_default_schema_and_config() {
        let tool = ToolDef::video("vid", "d", "openai", "sora-2", None, HashMap::new());
        assert_eq!(tool.tool_type, ToolType::GenerateVideo);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("GENERATE_VIDEO".to_owned()))
        );
        assert!(tool.input_schema["properties"]["duration"].is_object());
        assert_eq!(tool.input_schema["required"], serde_json::json!(["prompt"]));
    }

    #[test]
    fn test_pdf_tool_default_schema_and_config() {
        let tool = ToolDef::pdf(
            "generate_pdf",
            "Generate a PDF document from markdown text.",
            None,
            HashMap::new(),
        );
        assert_eq!(tool.tool_type, ToolType::GeneratePdf);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("GENERATE_PDF".to_owned()))
        );
        // No llmProvider/model -- pdf generation needs no AI provider.
        assert!(!tool.config.contains_key("llmProvider"));
        assert!(tool.input_schema["properties"]["markdown"].is_object());
    }

    #[test]
    fn test_pdf_tool_merges_extra_defaults() {
        let mut defaults = HashMap::new();
        defaults.insert("pageSize".to_owned(), Value::String("LETTER".to_owned()));
        let tool = ToolDef::pdf("t", "d", None, defaults);
        assert_eq!(
            tool.config.get("pageSize"),
            Some(&Value::String("LETTER".to_owned()))
        );
    }

    #[test]
    fn test_rag_index_tool_default_namespace_and_schema() {
        let tool = ToolDef::rag_index(
            "index_document",
            "d",
            "pgvectordb",
            "product_docs",
            "openai",
            "text-embedding-3-small",
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(tool.tool_type, ToolType::RagIndex);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("LLM_INDEX_TEXT".to_owned()))
        );
        assert_eq!(
            tool.config.get("namespace"),
            Some(&Value::String("default_ns".to_owned()))
        );
        assert_eq!(
            tool.config.get("vectorDB"),
            Some(&Value::String("pgvectordb".to_owned()))
        );
        assert_eq!(
            tool.input_schema["required"],
            serde_json::json!(["text", "docId"])
        );
        assert!(!tool.config.contains_key("chunkSize"));
    }

    #[test]
    fn test_rag_index_tool_optional_fields_when_given() {
        let tool = ToolDef::rag_index(
            "t",
            "d",
            "pgvectordb",
            "docs",
            "openai",
            "text-embedding-3-small",
            Some("custom_ns".to_owned()),
            Some(500),
            Some(50),
            Some(1536),
            None,
        );
        assert_eq!(
            tool.config.get("namespace"),
            Some(&Value::String("custom_ns".to_owned()))
        );
        assert_eq!(tool.config.get("chunkSize"), Some(&Value::from(500)));
        assert_eq!(tool.config.get("chunkOverlap"), Some(&Value::from(50)));
        assert_eq!(tool.config.get("dimensions"), Some(&Value::from(1536)));
    }

    #[test]
    fn test_rag_search_tool_default_max_results_and_namespace() {
        let tool = ToolDef::rag_search(
            "search_kb",
            "d",
            "pgvectordb",
            "product_docs",
            "openai",
            "text-embedding-3-small",
            None,
            None,
            None,
            None,
        );
        assert_eq!(tool.tool_type, ToolType::RagSearch);
        assert_eq!(
            tool.config.get("taskType"),
            Some(&Value::String("LLM_SEARCH_INDEX".to_owned()))
        );
        assert_eq!(tool.config.get("maxResults"), Some(&Value::from(5)));
        assert_eq!(
            tool.config.get("namespace"),
            Some(&Value::String("default_ns".to_owned()))
        );
        assert_eq!(tool.input_schema["required"], serde_json::json!(["query"]));
    }

    #[test]
    fn test_rag_search_tool_custom_max_results() {
        let tool = ToolDef::rag_search(
            "t",
            "d",
            "pgvectordb",
            "docs",
            "openai",
            "text-embedding-3-small",
            None,
            Some(10),
            None,
            None,
        );
        assert_eq!(tool.config.get("maxResults"), Some(&Value::from(10)));
    }

    #[test]
    fn test_wait_for_message_tool_blocking_default() {
        let tool = ToolDef::wait_for_message("wait_for_message", "d", 1, true);
        assert_eq!(tool.tool_type, ToolType::PullWorkflowMessages);
        assert_eq!(tool.config.get("batchSize"), Some(&Value::from(1)));
        assert!(!tool.config.contains_key("blocking"));
        assert_eq!(
            tool.input_schema,
            serde_json::json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn test_wait_for_message_tool_non_blocking_emits_false() {
        let tool = ToolDef::wait_for_message("t", "d", 5, false);
        assert_eq!(tool.config.get("batchSize"), Some(&Value::from(5)));
        assert_eq!(tool.config.get("blocking"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_tool_type_as_str_matches_python_wire_strings() {
        assert_eq!(ToolType::Api.as_str(), "api");
        assert_eq!(ToolType::GenerateImage.as_str(), "generate_image");
        assert_eq!(ToolType::GenerateAudio.as_str(), "generate_audio");
        assert_eq!(ToolType::GenerateVideo.as_str(), "generate_video");
        assert_eq!(ToolType::GeneratePdf.as_str(), "generate_pdf");
        assert_eq!(ToolType::RagIndex.as_str(), "rag_index");
        assert_eq!(ToolType::RagSearch.as_str(), "rag_search");
        assert_eq!(
            ToolType::PullWorkflowMessages.as_str(),
            "pull_workflow_messages"
        );
    }
}
