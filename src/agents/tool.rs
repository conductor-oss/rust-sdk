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

/// Tool invocation mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    Worker,
    Http,
    Mcp,
    AgentTool,
    Human,
}

impl ToolType {
    /// Wire-format string, matching python-sdk's `ToolType(str, Enum)` values exactly.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolType::Worker => "worker",
            ToolType::Http => "http",
            ToolType::Mcp => "mcp",
            ToolType::AgentTool => "agent_tool",
            ToolType::Human => "human",
        }
    }
}

/// Boxed async tool handler: takes the raw JSON arguments plus the resolved [`Credentials`] for
/// this call, returns raw JSON output.
///
/// Kept as raw `Value -> Value` (rather than generic over `T`) so `ToolDef` itself can stay
/// non-generic and be stored in a plain `Vec<ToolDef>` on [`AgentDef`]. [`ToolDef::function`] and
/// [`ToolDef::function_with_credentials`] are the generic entry points that wrap a strongly-typed
/// `Fn(T) -> Fut` / `Fn(T, &Credentials) -> Fut` into this shape — `function`'s wrapper simply
/// ignores the `Credentials` argument, so both constructors produce the same `ToolHandler` shape
/// and a caller invoking a tool never needs to know which constructor built it.
pub type ToolHandler = Arc<
    dyn Fn(Value, Credentials) -> Pin<Box<dyn Future<Output = Result<Value>> + Send>> + Send + Sync,
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
        tool.handler = Some(Arc::new(move |raw: Value, _credentials: Credentials| {
            let handler = handler.clone();
            Box::pin(async move {
                let args: T = serde_json::from_value(raw)?;
                handler(args).await
            })
        }));
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
        tool.handler = Some(Arc::new(move |raw: Value, credentials: Credentials| {
            let handler = handler.clone();
            Box::pin(async move {
                let args: T = serde_json::from_value(raw)?;
                handler(args, &credentials).await
            })
        }));
        tool
    }

    /// An HTTP-backed tool. `url` and `headers` values may reference declared credentials via
    /// `${NAME}` placeholders; any placeholder not present in `credentials` is rejected.
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
            &HashMap::from([("url".to_string(), url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Http);
        tool.credentials = credentials;
        tool.config.insert("url".to_string(), Value::String(url));
        tool.config
            .insert("method".to_string(), Value::String(method.to_uppercase()));
        tool.config
            .insert("headers".to_string(), serde_json::to_value(&headers)?);
        tool.config.insert(
            "accept".to_string(),
            Value::Array(vec![Value::String("application/json".to_string())]),
        );
        tool.config.insert(
            "contentType".to_string(),
            Value::String("application/json".to_string()),
        );
        Ok(tool)
    }

    /// An MCP-backed tool. Same `${NAME}` credential-placeholder validation as [`ToolDef::http`].
    pub fn mcp(
        server_url: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        headers: HashMap<String, String>,
        credentials: Vec<String>,
    ) -> Result<Self> {
        let server_url = server_url.into();
        validate_credential_placeholders(&headers, &credentials)?;
        validate_credential_placeholders(
            &HashMap::from([("server_url".to_string(), server_url.clone())]),
            &credentials,
        )?;

        let mut tool = Self::base(name, description, ToolType::Mcp);
        tool.credentials = credentials;
        tool.config
            .insert("server_url".to_string(), Value::String(server_url));
        tool.config
            .insert("headers".to_string(), serde_json::to_value(&headers)?);
        Ok(tool)
    }

    /// A tool that delegates to a sub-agent (`toolType: "agent_tool"`), recursively serialized
    /// into `config.agentConfig` by [`super::AgentConfigSerializer`].
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

    pub fn with_output_schema(mut self, schema: Value) -> Self {
        self.output_schema = schema;
        self
    }

    pub fn with_approval_required(mut self, approval_required: bool) -> Self {
        self.approval_required = approval_required;
        self
    }

    pub fn with_stateful(mut self, stateful: bool) -> Self {
        self.stateful = stateful;
        self
    }

    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

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
    pub fn with_credentials(mut self, credentials: Vec<String>) -> Self {
        self.credentials = credentials;
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
        let result = handler(serde_json::json!({"n": 21}), Credentials::default())
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
                let token = creds.get("API_KEY").unwrap().to_string();
                async move { Ok(Value::from(format!("{token}:{}", args.n * 2))) }
            },
        )
        .with_credentials(vec!["API_KEY".to_string()]);
        assert_eq!(tool.credentials, vec!["API_KEY".to_string()]);

        let mut values = HashMap::new();
        values.insert("API_KEY".to_string(), "secret".to_string());
        let creds = Credentials::new(values);

        let handler = tool.handler.clone().unwrap();
        let result = handler(serde_json::json!({"n": 21}), creds).await.unwrap();
        assert_eq!(result, Value::from("secret:42"));
    }

    #[test]
    fn test_http_tool_rejects_undeclared_placeholder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer ${API_KEY}".to_string());
        let result = ToolDef::http("t", "d", "https://example.com", "get", headers, vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_http_tool_accepts_declared_placeholder() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer ${API_KEY}".to_string());
        let result = ToolDef::http(
            "t",
            "d",
            "https://example.com",
            "get",
            headers,
            vec!["API_KEY".to_string()],
        );
        assert!(result.is_ok());
        let tool = result.unwrap();
        assert_eq!(
            tool.config.get("method"),
            Some(&Value::String("GET".to_string()))
        );
    }

    #[test]
    fn test_agent_tool_holds_sub_agent() {
        let sub = AgentDef::new("researcher").unwrap();
        let tool = ToolDef::agent(sub);
        assert_eq!(tool.tool_type, ToolType::AgentTool);
        assert!(tool.sub_agent.is_some());
    }
}
