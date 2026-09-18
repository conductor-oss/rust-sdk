// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

// OCG (Open Context Graph) retrieval sub-agent.
//
// OCG is a retrieval engine over a knowledge graph of entities (messages, channels, people)
// linked by claims and relationships. Every tool compiles to a plain Conductor HTTP task with
// URI templating (see `super::tool::ToolDef::http_templated`) — there is no OCG-specific
// server code.
//
// `url` is required for `ocg_tools`/`ocg_agent` — there is no server-side default.
// `credential` names an entry in the server's credential store; the secret itself never
// appears in Rust code or serialized configs.
//
// Agents bound to different OCG instances must have distinct names: inline `agent_tool` child
// workflows are registered by agent name, so two differently-configured agents sharing a name
// would overwrite each other's workflow definition.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::error::Result;

use super::def::AgentDef;
use super::tool::ToolDef;

/// `${workflow.input.__today__}` is substituted by Conductor with the current UTC date when
/// the LLM task is scheduled, so relative-date queries anchor on the real current date.
pub const OCG_SYSTEM_PROMPT: &str = "\
Today's date is ${workflow.input.__today__} (UTC). DEFAULT: do NOT set
start_time or end_time \u{2014} OMIT BOTH ENTIRELY. Searching the full history is
the norm, and an unrequested time range silently drops older context that
is usually exactly what you need. ONLY add a time range when the user
EXPLICITLY asks about a recent or time-bounded window (\"recent\", \"last
week\", \"since Friday\", \"in May\"); then anchor on today's date and set
start_time (and set end_time only for a window that closed in the past).
Timestamps must be full RFC3339 (2026-06-04T00:00:00Z); a bare date is
rejected. Never invent a range the user did not ask for.

You are querying an OCG (Open Context Graph). It is a RETRIEVAL
engine over a knowledge graph of entities (messages, channels, people)
linked by claims and relationships \u{2014} embedding/keyword search, NOT an LLM.
It is NOT an aggregation engine and NOT a conversation partner: rephrasing
the same intent returns the same results.

RETRIEVAL BUDGET: make at most 3 queries total, each with a genuinely
DIFFERENT keyword set. Never repeat or lightly rephrase a query. When the
budget is spent \u{2014} or results start repeating \u{2014} STOP querying and answer
from what you have. Timestamps must be full RFC3339
(2026-06-04T00:00:00Z); a bare date is rejected.

It can answer:
  - \"Find messages in channel X about Y\"
  - \"Show TIMED_OUT errors for cluster <name>\"
  - \"What entities mention 'health check failure'?\"
  - \"Recent messages in #cloud_saas_health_check_alerts\"

It CANNOT directly answer (you must do it yourself in two steps):
  - \"How many of X are there?\" / \"Which X is most frequent?\"
  - \"Group these by Y\" / \"Top N by count\"
  - Statistical or comparative questions

RESPONSE SIZE: ALWAYS request max_results=100 \u{2014} it is both the maximum and
the floor for getting decent context; NEVER use a small value like 10 or
25, which starves your answer. ALWAYS set traversal_level = 1 (never 0,
never higher). To focus results, sharpen the KEYWORDS \u{2014} never by lowering
max_results or by adding an unrequested time range.

DIG DEEPER: the first ocg_query is only your entry point. After it returns,
pick the 1-3 MOST RELEVANT entities from the citations (the ones most on
point for the question) and call ocg_neighborhood on each \u{2014} using the
entity ids from the citation rows \u{2014} to pull in their linked entities
(related tickets, incidents, sub-workflows, prior fixes). The actual fix
very often lives one hop away in a linked entity, not in the first page of
citations. Do not answer from the initial citations alone when a clearly
relevant entity is worth expanding.

For aggregation questions, use a TWO-STEP pattern:
  1. RETRIEVE: ask OCG for the relevant entities.
     - Use specific terms (cluster names, error codes, channel names).
     - Use start_time (and end_time only for windows closed in the
       past) to bound the range.
     - Avoid hedging words (\"frequently\", \"across\", \"occurrences\") \u{2014}
       OCG ranks by keyword presence, and these are noise tokens.
  2. AGGREGATE: count, group, rank yourself from the citation list.

Query length: keep it under ~15 content words. Long prompts dilute the
BM25 keyword set; OCG's parser is extracting things like \"happen\",
\"identify\", \"top one\" which are not real signal.

Bad:  \"Across all clusters, what alert/notification/error type appears
       most frequently? Group similar alerts and tell me which one has
       the highest count and how many clusters it affected.\"

Good (step 1): {
  \"query\": \"TIMED_OUT health check failure cluster\",
  \"max_results\": 100
}
(no start_time/end_time \u{2014} the query runs across the full history.)
Then parse the returned citations, extract cluster names from titles,
build the frequency table in your reasoning.";

// One OCG tool's endpoint shape, resolved into a `ToolDef` by `ocg_tools` once the common
// `base_url`/`headers`/`credentials` are known.
struct OcgToolSpec {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    description: &'static str,
    schema: Value,
    query_params: Option<Vec<&'static str>>,
}

fn query_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_query",
        method: "POST",
        path: "/api/v1/agent/query",
        description: "Query the Open Context Graph for structured retrieval. Returns citations \
            (source_item_id, title, container_id, snippet) and traversal_results when \
            traversal_level > 0.",
        schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Natural-language retrieval query."},
                "max_results": {
                    "type": "integer",
                    "description": "Max citations to return. ALWAYS use 100 \u{2014} it is both the \
                        hard maximum and the floor for getting decent context; never request \
                        fewer.",
                    "default": 100,
                    "minimum": 100,
                    "maximum": 100,
                },
                "traversal_level": {
                    "type": "integer",
                    "description": "ALWAYS 1 \u{2014} pulls each citation's immediate neighborhood in \
                        alongside it. Never 0 (too shallow) and never higher (dig deeper by \
                        calling ocg_neighborhood on the most relevant entities, not by raising \
                        this).",
                    "default": 1,
                    "minimum": 1,
                    "maximum": 1,
                },
                "start_time": {
                    "type": "string",
                    "description": "LEAVE UNSET by default \u{2014} omit it so the search covers the \
                        FULL history. Set this ONLY when the user EXPLICITLY asked about a \
                        recent or time-bounded window (e.g. 'last week', 'since Friday'). \
                        RFC3339 lower bound (inclusive), e.g. 2026-06-04T00:00:00Z; a bare date \
                        like 2026-06-04 is REJECTED.",
                },
                "end_time": {
                    "type": "string",
                    "description": "LEAVE UNSET by default. Set this ONLY for a window the \
                        user said has CLOSED in the past; for anything running through now, \
                        omit it. RFC3339 upper bound (exclusive), e.g. 2026-06-11T00:00:00Z; a \
                        bare date is REJECTED.",
                },
            },
            "required": ["query"],
        }),
        query_params: None,
    }
}

fn get_entity_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_get_entity",
        method: "GET",
        path: "/api/v1/entities/{entity_id}",
        description: "Fetch one entity by its canonical id.",
        schema: json!({
            "type": "object",
            "properties": {
                "entity_id": {
                    "type": "string",
                    "description": "Canonical entity id from an ocg_query result row.",
                },
            },
            "required": ["entity_id"],
        }),
        query_params: None,
    }
}

fn neighborhood_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_neighborhood",
        method: "GET",
        path: "/api/v1/graph/neighborhood/{entity_id}",
        description: "Get an entity plus its graph neighbors out to `depth` hops. Use limit \
            <= 10, depth=1 on the first call \u{2014} well-connected entities can have many edges and \
            large responses will be truncated.",
        schema: json!({
            "type": "object",
            "properties": {
                "entity_id": {
                    "type": "string",
                    "description": "Entity at the center of the neighborhood.",
                },
                "depth": {
                    "type": "integer",
                    "description": "Hop depth (use depth=1 on first call).",
                    "default": 1,
                },
                "limit": {
                    "type": "integer",
                    "description": "Cap on neighbors returned (use <= 10 on first call).",
                    "default": 50,
                },
            },
            "required": ["entity_id"],
        }),
        query_params: Some(vec!["depth", "limit"]),
    }
}

fn memory_set_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_memory_set",
        method: "POST",
        path: "/api/v1/memories",
        description: "Create or overwrite a memory in OCG. Cap inferred confidence at 0.7; \
            never write PII or secrets.",
        schema: json!({
            "type": "object",
            "properties": {
                "key": {"type": "string", "description": "Memory key."},
                "agent": {"type": "string", "description": "Agent owner (e.g. \"agent:<name>\")."},
                "user": {"type": "string", "description": "User owner (e.g. \"user:<name>\")."},
                "string_value": {"type": "string", "description": "Stored value."},
                "description": {"type": "string", "description": "Human-readable description."},
                "scope": {
                    "type": "string",
                    "description": "Memory scope. One of MEMORY_SCOPE_SESSION, \
                        MEMORY_SCOPE_AGENT, MEMORY_SCOPE_USER, MEMORY_SCOPE_SHARED, \
                        MEMORY_SCOPE_GLOBAL.",
                    "default": "MEMORY_SCOPE_USER",
                },
                "confidence": {
                    "type": "number",
                    "description": "Inferred confidence in [0,1]. Cap at 0.7.",
                    "default": 0.7,
                },
                "source_ref": {
                    "type": "string",
                    "description": "Free-form source reference (e.g. message id).",
                },
                "evidence_ids": {
                    "type": "array",
                    "description": "Supporting evidence entity ids.",
                    "items": {"type": "string"},
                },
                "tags": {
                    "type": "array",
                    "description": "Tags.",
                    "items": {"type": "string"},
                },
                "expires_at": {
                    "type": "string",
                    "description": "ISO-8601 expiry. Optional \u{2014} default 180 days.",
                },
                "idempotency_key": {"type": "string", "description": "Idempotency key. Optional."},
            },
            "required": ["key", "agent", "user", "string_value", "description"],
        }),
        query_params: None,
    }
}

fn memory_reinforce_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_memory_reinforce",
        method: "POST",
        path: "/api/v1/memories/{key}/reinforce",
        description: "Reinforce an existing memory on independent re-observation. \
            confidence_boost must be <= 0.05.",
        schema: json!({
            "type": "object",
            "properties": {
                "key": {"type": "string", "description": "Memory key."},
                "agent": {"type": "string", "description": "Agent owner."},
                "user": {"type": "string", "description": "User owner."},
                "confidence_boost": {
                    "type": "number",
                    "description": "Boost to add (must be <= 0.05 to prevent compounding drift).",
                    "default": 0.05,
                },
                "source_ref": {"type": "string", "description": "Free-form source reference."},
            },
            "required": ["key", "agent", "user"],
        }),
        query_params: None,
    }
}

fn memory_delete_tool_spec() -> OcgToolSpec {
    OcgToolSpec {
        name: "ocg_memory_delete",
        method: "DELETE",
        path: "/api/v1/memories/{key}",
        description: "Delete a memory by key. Prefer ocg_memory_set with a corrected value over \
            deletion (preserves history).",
        schema: json!({
            "type": "object",
            "properties": {
                "key": {"type": "string", "description": "Memory key."},
                "agent": {"type": "string", "description": "Agent owner."},
                "user": {"type": "string", "description": "User owner."},
            },
            "required": ["key", "agent", "user"],
        }),
        query_params: Some(vec!["agent", "user"]),
    }
}

/// Which OCG tool groups to include. All default to `true`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcgToolSelection {
    /// Include `ocg_query`.
    pub query: bool,
    /// Include `ocg_get_entity` + `ocg_neighborhood`.
    pub entities: bool,
    /// Include `ocg_memory_set` / `ocg_memory_reinforce` / `ocg_memory_delete`.
    pub memory: bool,
}

impl Default for OcgToolSelection {
    fn default() -> Self {
        Self {
            query: true,
            entities: true,
            memory: true,
        }
    }
}

/// Build the raw OCG [`ToolDef`] list for a custom retrieval agent. Each tool is a plain
/// Conductor HTTP task; the LLM's arguments fill the path/query/body at call time.
///
/// `credential` names a credential-store entry holding the OCG bearer token; the server
/// resolves it at execution time, so the secret never appears in the serialized config.
///
/// # Errors
///
/// Returns [`crate::error::ConductorError::Agent`] if `url` is blank.
pub fn ocg_tools(
    url: impl Into<String>,
    credential: Option<&str>,
    selection: OcgToolSelection,
) -> Result<Vec<ToolDef>> {
    let url = url.into();
    if url.trim().is_empty() {
        return Err(crate::error::ConductorError::agent(
            "ocg_tools() requires a non-blank url: every OCG tool set binds its own instance.",
        ));
    }
    let base_url = url.trim().trim_end_matches('/').to_owned();

    let mut headers: HashMap<String, String> = HashMap::new();
    let mut credentials: Vec<String> = Vec::new();
    if let Some(credential) = credential {
        // Standard http-tool placeholder — resolved server-side from the credential store at
        // execution; the token never appears here.
        headers.insert(
            "Authorization".to_owned(),
            format!("Bearer ${{{credential}}}"),
        );
        credentials.push(credential.to_owned());
    }

    let mut specs = Vec::new();
    if selection.query {
        specs.push(query_tool_spec());
    }
    if selection.entities {
        specs.push(get_entity_tool_spec());
        specs.push(neighborhood_tool_spec());
    }
    if selection.memory {
        specs.push(memory_set_tool_spec());
        specs.push(memory_reinforce_tool_spec());
        specs.push(memory_delete_tool_spec());
    }

    specs
        .into_iter()
        .map(|spec| {
            ToolDef::http_templated(
                spec.name,
                spec.description,
                spec.schema,
                base_url.clone(),
                spec.method,
                Some(spec.path.to_owned()),
                spec.query_params
                    .map(|params| params.into_iter().map(String::from).collect()),
                headers.clone(),
                credentials.clone(),
            )
        })
        .collect()
}

/// Options for [`ocg_agent`].
#[derive(Debug, Clone)]
pub struct OcgAgentOptions {
    /// Agent name. **Must be distinct per OCG instance** — child workflows are registered by
    /// agent name (see module doc). Defaults to `"ocg_agent"`.
    pub name: String,
    /// Credential-store entry for the instance's bearer token.
    pub credential: Option<String>,
    /// Override the canned [`OCG_SYSTEM_PROMPT`].
    pub instructions: Option<String>,
    /// Retrieval loop budget. Defaults to `10`.
    pub max_turns: u32,
    /// Tool subset switches, forwarded to [`ocg_tools`].
    pub tool_selection: OcgToolSelection,
}

impl Default for OcgAgentOptions {
    fn default() -> Self {
        Self {
            name: "ocg_agent".to_owned(),
            credential: None,
            instructions: None,
            max_turns: 10,
            tool_selection: OcgToolSelection::default(),
        }
    }
}

/// Build the prebuilt OCG retrieval [`AgentDef`]. Wrap it with [`ToolDef::agent`] to let a main
/// agent delegate retrieval, or use it as a pipeline stage before the main agent runs.
///
/// `model` is the LLM for the retrieval agent's own turns (required). `url` is the OCG
/// instance base URL (required — no server-side default).
///
/// # Errors
///
/// Returns [`crate::error::ConductorError::Agent`] if `url` is blank, or if `options.name` is
/// empty or invalid.
pub fn ocg_agent(
    model: impl Into<String>,
    url: impl Into<String>,
    options: OcgAgentOptions,
) -> Result<AgentDef> {
    let tools = ocg_tools(url, options.credential.as_deref(), options.tool_selection)?;
    let instructions = options
        .instructions
        .unwrap_or_else(|| OCG_SYSTEM_PROMPT.to_owned());
    AgentDef::new(options.name)?
        .with_model(model)
        .with_instructions(instructions)
        .with_tools(tools)
        .with_max_turns(options.max_turns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocg_tools_rejects_blank_url() {
        let err = ocg_tools("", None, OcgToolSelection::default()).unwrap_err();
        assert!(err.to_string().contains("requires a non-blank url"));
    }

    #[test]
    fn test_ocg_tools_rejects_whitespace_only_url() {
        let err = ocg_tools("   ", None, OcgToolSelection::default()).unwrap_err();
        assert!(err.to_string().contains("requires a non-blank url"));
    }

    #[test]
    fn test_ocg_tools_default_selection_returns_all_six() {
        let tools =
            ocg_tools("https://ocg.example.com", None, OcgToolSelection::default()).unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "ocg_query",
                "ocg_get_entity",
                "ocg_neighborhood",
                "ocg_memory_set",
                "ocg_memory_reinforce",
                "ocg_memory_delete",
            ]
        );
    }

    #[test]
    fn test_ocg_tools_query_only() {
        let selection = OcgToolSelection {
            query: true,
            entities: false,
            memory: false,
        };
        let tools = ocg_tools("https://ocg.example.com", None, selection).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ocg_query");
    }

    #[test]
    fn test_ocg_tools_strips_trailing_slash_from_url() {
        let tools = ocg_tools(
            "https://ocg.example.com/",
            None,
            OcgToolSelection {
                query: true,
                entities: false,
                memory: false,
            },
        )
        .unwrap();
        assert_eq!(
            tools[0].config.get("url"),
            Some(&Value::String("https://ocg.example.com".to_owned()))
        );
    }

    #[test]
    fn test_ocg_tools_sets_path_template_and_method() {
        let tools = ocg_tools(
            "https://ocg.example.com",
            None,
            OcgToolSelection {
                query: true,
                entities: false,
                memory: false,
            },
        )
        .unwrap();
        assert_eq!(
            tools[0].config.get("pathTemplate"),
            Some(&Value::String("/api/v1/agent/query".to_owned()))
        );
        assert_eq!(
            tools[0].config.get("method"),
            Some(&Value::String("POST".to_owned()))
        );
    }

    #[test]
    fn test_ocg_tools_neighborhood_sets_query_params() {
        let tools = ocg_tools(
            "https://ocg.example.com",
            None,
            OcgToolSelection {
                query: false,
                entities: true,
                memory: false,
            },
        )
        .unwrap();
        let neighborhood = tools.iter().find(|t| t.name == "ocg_neighborhood").unwrap();
        assert_eq!(
            neighborhood.config.get("queryParams"),
            Some(&json!(["depth", "limit"]))
        );
    }

    #[test]
    fn test_ocg_tools_without_credential_has_no_headers_or_credentials() {
        let tools = ocg_tools(
            "https://ocg.example.com",
            None,
            OcgToolSelection {
                query: true,
                entities: false,
                memory: false,
            },
        )
        .unwrap();
        assert!(!tools[0].config.contains_key("headers"));
        assert!(tools[0].credentials.is_empty());
    }

    #[test]
    fn test_ocg_tools_with_credential_sets_bearer_header_and_credentials() {
        let tools = ocg_tools(
            "https://ocg.example.com",
            Some("OCG_KEY"),
            OcgToolSelection {
                query: true,
                entities: false,
                memory: false,
            },
        )
        .unwrap();
        assert_eq!(
            tools[0].config.get("headers"),
            Some(&json!({"Authorization": "Bearer ${OCG_KEY}"}))
        );
        assert_eq!(tools[0].credentials, vec!["OCG_KEY".to_owned()]);
    }

    #[test]
    fn test_ocg_agent_defaults() {
        let agent = ocg_agent(
            "anthropic/claude-sonnet-4-6",
            "https://ocg.example.com",
            OcgAgentOptions::default(),
        )
        .unwrap();
        assert_eq!(agent.name, "ocg_agent");
        assert_eq!(agent.model, Some("anthropic/claude-sonnet-4-6".to_owned()));
        assert_eq!(agent.instructions, Some(OCG_SYSTEM_PROMPT.to_owned()));
        assert_eq!(agent.max_turns, 10);
        assert_eq!(agent.tools.len(), 6);
    }

    #[test]
    fn test_ocg_agent_custom_name_and_instructions() {
        let agent = ocg_agent(
            "anthropic/claude-sonnet-4-6",
            "https://us.ocg.example.com",
            OcgAgentOptions {
                name: "ocg_us".to_owned(),
                instructions: Some("custom prompt".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(agent.name, "ocg_us");
        assert_eq!(agent.instructions, Some("custom prompt".to_owned()));
    }

    #[test]
    fn test_ocg_agent_propagates_blank_url_error() {
        let err = ocg_agent(
            "anthropic/claude-sonnet-4-6",
            "",
            OcgAgentOptions::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("requires a non-blank url"));
    }

    // Pins the exact prompt text (length + interior anchors) so an edit that breaks the
    // wrapping is caught here.
    #[test]
    fn test_ocg_system_prompt_matches_python_exactly() {
        // str::len() counts UTF-8 bytes: the prompt has 3738 chars including 10 em-dashes (3
        // bytes each), hence 3758 bytes.
        assert_eq!(OCG_SYSTEM_PROMPT.len(), 3758);
        assert_eq!(OCG_SYSTEM_PROMPT.chars().count(), 3738);
        assert!(OCG_SYSTEM_PROMPT.starts_with(
            "Today's date is ${workflow.input.__today__} (UTC). DEFAULT: do NOT set\nstart_time"
        ));
        assert!(OCG_SYSTEM_PROMPT.contains(
            "RETRIEVAL BUDGET: make at most 3 queries total, each with a genuinely\nDIFFERENT"
        ));
        assert!(OCG_SYSTEM_PROMPT.ends_with(
            "Then parse the returned citations, extract cluster names from titles,\nbuild the frequency table in your reasoning."
        ));
    }
}
