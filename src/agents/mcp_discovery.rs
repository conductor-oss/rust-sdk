// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! MCP tool discovery — discovers individual tools from a live MCP server at compile time.
//! Ports python-sdk's `runtime/mcp_discovery.py`.
//!
//! ## Not wired into anything — matching python exactly, not a gap
//!
//! Confirmed by reading python-sdk directly: `discover_mcp_tools`/`expand_mcp_tool_def` are
//! defined in `mcp_discovery.py` but a repo-wide `grep` turns up zero call sites for either
//! anywhere else in python-sdk — not in `runtime.py`, not in any example. The parity audit's
//! original finding characterizing this as "Python runs a `LIST_MCP_TOOLS` system task at
//! compile time and expands one `mcp_tool()` call into N real per-tool schemas... Rust's
//! `ToolDef::mcp()` always produces one static, opaque tool definition" overstated python's
//! *actual* behavior: `mcp_tool()` itself only ever builds the same static, unexpanded
//! `{"server_url", "headers"?, "tool_names"?, "max_tools"}` config both SDKs already produce
//! identically. `mcp_discovery.py` is real, working, but dead code on the python side too — a
//! capability nothing currently calls. This module ports that same capability (a caller can
//! invoke [`discover_mcp_tools`]/[`expand_mcp_tool_def`] explicitly before handing tools to
//! [`super::AgentDef::with_tool`]), not a live pipeline wired into [`super::AgentRuntime`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use serde_json::Value;

use crate::client::WorkflowClient;
use crate::error::{ConductorError, Result};
use crate::models::{StartWorkflowRequest, WorkflowDef, WorkflowTask};

use super::tool::{ToolDef, ToolType};

/// One tool descriptor returned by a `LIST_MCP_TOOLS` task, matching python's discovered-tool
/// dict shape (`{"name", "description", "inputSchema"}`).
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

static DISCOVERY_CACHE: Lazy<Mutex<HashMap<String, Vec<DiscoveredMcpTool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

async fn fetch_mcp_tools(
    workflow_client: &WorkflowClient,
    server_url: &str,
    headers: Option<HashMap<String, String>>,
) -> Result<Vec<DiscoveredMcpTool>> {
    let task = WorkflowTask::list_mcp_tools("list_tools", server_url, headers);

    let mut output_parameters = HashMap::new();
    output_parameters.insert(
        "tools".to_string(),
        Value::String("${list_tools.output.tools}".to_string()),
    );

    let workflow_def = WorkflowDef {
        name: "__mcp_discovery__".to_string(),
        version: 1,
        description: Some("MCP tool discovery (ephemeral)".to_string()),
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
                .unwrap_or_else(|| "unknown".to_string())
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
            let name = t.get("name").and_then(Value::as_str)?.to_string();
            Some(DiscoveredMcpTool {
                name,
                description: t
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                input_schema: t
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Default::default())),
            })
        })
        .collect())
}

/// Discover tools from an MCP server via a `LIST_MCP_TOOLS` task, matching python's
/// `discover_mcp_tools`: builds a minimal one-task ephemeral workflow, executes it
/// synchronously, and returns the discovered tool list. Results are cached per `server_url` —
/// call [`clear_mcp_discovery_cache`] to force a re-fetch. Returns an empty list on any failure
/// (network error, workflow failure, timeout) — a graceful fallback matching python's own
/// broad `except Exception: return []`, not an error a caller needs to handle.
pub async fn discover_mcp_tools(
    workflow_client: &WorkflowClient,
    server_url: &str,
    headers: Option<HashMap<String, String>>,
) -> Vec<DiscoveredMcpTool> {
    if let Some(cached) = DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(server_url)
    {
        return cached.clone();
    }

    let discovered = fetch_mcp_tools(workflow_client, server_url, headers)
        .await
        .unwrap_or_default();

    DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(server_url.to_string(), discovered.clone());
    discovered
}

/// Clear the MCP discovery cache, matching python's `clear_discovery_cache` — useful in tests
/// or when an MCP server's tools change.
pub fn clear_mcp_discovery_cache() {
    DISCOVERY_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// Expand a single MCP [`ToolDef`] (from [`ToolDef::mcp`]) into one [`ToolDef`] per discovered
/// tool, matching python's `expand_mcp_tool_def`: each carries the correct name/description/
/// input schema while inheriting the original `server_url`/`headers`/`max_tools` config. Honors
/// a `tool_names` whitelist if the original tool's config set one. Falls back to `[mcp_td]`
/// unchanged if nothing was discovered or everything was filtered out — same graceful-fallback
/// contract as python.
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
            config.insert("server_url".to_string(), server_url.clone());
            config.insert("max_tools".to_string(), max_tools.clone());
            if let Some(headers) = &headers {
                config.insert("headers".to_string(), headers.clone());
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
            name: "mcp_tools".to_string(),
            description: "MCP tools".to_string(),
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
            name: name.to_string(),
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
            "server_url".to_string(),
            Value::String("http://mcp".to_string()),
        );
        config.insert("max_tools".to_string(), Value::from(32));
        let original = mcp_tool_def(config);

        let discovered_tools = vec![discovered("search"), discovered("fetch")];
        let expanded = expand_mcp_tool_def(&original, &discovered_tools);

        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].name, "search");
        assert_eq!(expanded[0].tool_type, ToolType::Mcp);
        assert_eq!(
            expanded[0].config.get("server_url"),
            Some(&Value::String("http://mcp".to_string()))
        );
        assert_eq!(expanded[0].config.get("max_tools"), Some(&Value::from(32)));
        assert_eq!(expanded[1].name, "fetch");
    }

    #[test]
    fn test_expand_mcp_tool_def_inherits_headers_when_present() {
        let mut config = HashMap::new();
        config.insert(
            "server_url".to_string(),
            Value::String("http://mcp".to_string()),
        );
        config.insert(
            "headers".to_string(),
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
        config.insert("tool_names".to_string(), serde_json::json!(["search"]));
        let original = mcp_tool_def(config);

        let discovered_tools = vec![discovered("search"), discovered("fetch")];
        let expanded = expand_mcp_tool_def(&original, &discovered_tools);

        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].name, "search");
    }

    #[test]
    fn test_expand_mcp_tool_def_whitelist_matching_nothing_returns_original() {
        let mut config = HashMap::new();
        config.insert("tool_names".to_string(), serde_json::json!(["nonexistent"]));
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
