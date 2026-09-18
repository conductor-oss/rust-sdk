// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// MCP tool discovery — discovers individual tools from a live MCP server at compile time.
//
// Not wired into the runtime automatically: call discover_mcp_tools/expand_mcp_tool_def
// explicitly before handing tools to super::AgentDef::with_tool.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::client::WorkflowClient;
use crate::error::{ConductorError, Result};
use crate::models::{StartWorkflowRequest, WorkflowDef, WorkflowTask};

use super::tool::{ToolDef, ToolType};

/// One tool descriptor returned by a `LIST_MCP_TOOLS` task.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

static DISCOVERY_CACHE: std::sync::LazyLock<Mutex<HashMap<String, Vec<DiscoveredMcpTool>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

async fn fetch_mcp_tools(
    workflow_client: &WorkflowClient,
    server_url: &str,
    headers: Option<HashMap<String, String>>,
) -> Result<Vec<DiscoveredMcpTool>> {
    let task = WorkflowTask::list_mcp_tools("list_tools", server_url, headers);

    let mut output_parameters = HashMap::new();
    output_parameters.insert(
        "tools".to_owned(),
        Value::String("${list_tools.output.tools}".to_owned()),
    );

    let workflow_def = WorkflowDef {
        name: "__mcp_discovery__".to_owned(),
        version: 1,
        description: Some("MCP tool discovery (ephemeral)".to_owned()),
        tasks: vec![task],
        output_parameters,
        ..Default::default()
    };

    let mut request = StartWorkflowRequest::new("__mcp_discovery__");
    request.version = Some(1);
    request.workflow_def = Some(workflow_def);

    let run = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await?;

    if !run.is_successful() {
        return Err(ConductorError::agent(format!(
            "MCP discovery workflow failed for {server_url}: {}",
            run.reason_for_incompletion
                .unwrap_or_else(|| "unknown".to_owned())
        )));
    }

    let tools = run
        .output
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Ok(tools
        .into_iter()
        .filter_map(|t| {
            let name = t.get("name").and_then(Value::as_str)?.to_owned();
            Some(DiscoveredMcpTool {
                name,
                description: t
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                input_schema: t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(serde_json::Map::default())),
            })
        })
        .collect())
}

/// Discover tools from an MCP server via a `LIST_MCP_TOOLS` task: builds a minimal one-task
/// ephemeral workflow, executes it synchronously, and returns the discovered tool list. Results
/// are cached per `server_url` — call [`clear_mcp_discovery_cache`] to force a re-fetch. Returns
/// an empty list on any failure (network error, workflow failure, timeout) rather than an error.
pub async fn discover_mcp_tools(
    workflow_client: &WorkflowClient,
    server_url: &str,
    headers: Option<HashMap<String, String>>,
) -> Vec<DiscoveredMcpTool> {
    if let Some(cached) = DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(server_url)
    {
        return cached.clone();
    }

    let discovered = fetch_mcp_tools(workflow_client, server_url, headers)
        .await
        .unwrap_or_default();

    DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(server_url.to_owned(), discovered.clone());
    discovered
}

/// Clear the MCP discovery cache — useful in tests or when an MCP server's tools change.
pub fn clear_mcp_discovery_cache() {
    DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// Expand a single MCP [`ToolDef`] (from [`ToolDef::mcp`]) into one [`ToolDef`] per discovered
/// tool, each inheriting the original `server_url`/`headers`/`max_tools` config. Honors a
/// `tool_names` whitelist if the original tool's config set one. Falls back to `[mcp_td]`
/// unchanged if nothing was discovered or everything was filtered out.
#[must_use]
pub fn expand_mcp_tool_def(mcp_td: &ToolDef, discovered: &[DiscoveredMcpTool]) -> Vec<ToolDef> {
    if discovered.is_empty() {
        return vec![mcp_td.clone()];
    }

    let allowed_names: Option<Vec<&str>> = mcp_td.config.get("tool_names").and_then(|v| {
        v.as_array()
            .map(|arr| arr.iter().filter_map(Value::as_str).collect())
    });

    let filtered: Vec<&DiscoveredMcpTool> = match &allowed_names {
        Some(allowed) => discovered
            .iter()
            .filter(|t| allowed.contains(&t.name.as_str()))
            .collect(),
        None => discovered.iter().collect(),
    };

    if filtered.is_empty() {
        return vec![mcp_td.clone()];
    }

    let server_url = mcp_td
        .config
        .get("server_url")
        .cloned()
        .unwrap_or(Value::Null);
    let max_tools = mcp_td
        .config
        .get("max_tools")
        .cloned()
        .unwrap_or_else(|| Value::from(64));
    let headers = mcp_td.config.get("headers").cloned();

    filtered
        .into_iter()
        .map(|tool_info| {
            let mut config = HashMap::new();
            config.insert("server_url".to_owned(), server_url.clone());
            config.insert("max_tools".to_owned(), max_tools.clone());
            if let Some(headers) = &headers {
                config.insert("headers".to_owned(), headers.clone());
            }
            ToolDef {
                name: tool_info.name.clone(),
                description: tool_info.description.clone(),
                input_schema: tool_info.input_schema.clone(),
                output_schema: Value::Null,
                tool_type: ToolType::Mcp,
                approval_required: false,
                stateful: false,
                timeout_seconds: None,
                max_calls: None,
                config,
                credentials: Vec::new(),
                sub_agent: None,
                handler: None,
                guardrails: Vec::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mcp_tool_def(config: HashMap<String, Value>) -> ToolDef {
        ToolDef {
            name: "mcp_tools".to_owned(),
            description: "MCP tools".to_owned(),
            input_schema: Value::Null,
            output_schema: Value::Null,
            tool_type: ToolType::Mcp,
            approval_required: false,
            stateful: false,
            timeout_seconds: None,
            max_calls: None,
            config,
            credentials: Vec::new(),
            sub_agent: None,
            handler: None,
            guardrails: Vec::new(),
        }
    }

    fn discovered(name: &str) -> DiscoveredMcpTool {
        DiscoveredMcpTool {
            name: name.to_owned(),
            description: format!("{name} description"),
            input_schema: serde_json::json!({"type": "object"}),
        }
    }

    #[test]
    fn test_expand_mcp_tool_def_no_discovery_returns_original() {
        let original = mcp_tool_def(HashMap::new());
        let expanded = expand_mcp_tool_def(&original, &[]);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].name, original.name);
    }

    #[test]
    fn test_expand_mcp_tool_def_builds_one_tool_per_discovered() {
        let mut config = HashMap::new();
        config.insert(
            "server_url".to_owned(),
            Value::String("http://mcp".to_owned()),
        );
        config.insert("max_tools".to_owned(), Value::from(32));
        let original = mcp_tool_def(config);

        let discovered_tools = vec![discovered("search"), discovered("fetch")];
        let expanded = expand_mcp_tool_def(&original, &discovered_tools);

        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].name, "search");
        assert_eq!(expanded[0].tool_type, ToolType::Mcp);
        assert_eq!(
            expanded[0].config.get("server_url"),
            Some(&Value::String("http://mcp".to_owned()))
        );
        assert_eq!(expanded[0].config.get("max_tools"), Some(&Value::from(32)));
        assert_eq!(expanded[1].name, "fetch");
    }

    #[test]
    fn test_expand_mcp_tool_def_inherits_headers_when_present() {
        let mut config = HashMap::new();
        config.insert(
            "server_url".to_owned(),
            Value::String("http://mcp".to_owned()),
        );
        config.insert(
            "headers".to_owned(),
            serde_json::json!({"Authorization": "Bearer ${TOKEN}"}),
        );
        let original = mcp_tool_def(config);

        let expanded = expand_mcp_tool_def(&original, &[discovered("search")]);
        assert_eq!(
            expanded[0].config.get("headers"),
            Some(&serde_json::json!({"Authorization": "Bearer ${TOKEN}"}))
        );
    }

    #[test]
    fn test_expand_mcp_tool_def_applies_tool_names_whitelist() {
        let mut config = HashMap::new();
        config.insert("tool_names".to_owned(), serde_json::json!(["search"]));
        let original = mcp_tool_def(config);

        let discovered_tools = vec![discovered("search"), discovered("fetch")];
        let expanded = expand_mcp_tool_def(&original, &discovered_tools);

        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].name, "search");
    }

    #[test]
    fn test_expand_mcp_tool_def_whitelist_matching_nothing_returns_original() {
        let mut config = HashMap::new();
        config.insert("tool_names".to_owned(), serde_json::json!(["nonexistent"]));
        let original = mcp_tool_def(config);

        let discovered_tools = vec![discovered("search")];
        let expanded = expand_mcp_tool_def(&original, &discovered_tools);
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].name, original.name);
    }

    #[test]
    fn test_expand_mcp_tool_def_carries_input_schema_from_discovery() {
        let original = mcp_tool_def(HashMap::new());
        let expanded = expand_mcp_tool_def(&original, &[discovered("search")]);
        assert_eq!(
            expanded[0].input_schema,
            serde_json::json!({"type": "object"})
        );
    }

    #[test]
    fn test_clear_mcp_discovery_cache_runs_without_error() {
        clear_mcp_discovery_cache();
    }
}
