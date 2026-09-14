// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Agent Skills integration — ports python-sdk's `skill.py`: load an
//! [agentskills.io](https://agentskills.io) skill directory (a `SKILL.md` file plus optional
//! `*-agent.md`/`scripts/`/`references/`/`examples/`/`assets/` entries) as a runnable agent.
//!
//! ## Wire mechanism — a "framework" marker, not an ordinary [`AgentDef`]
//!
//! A loaded skill is **not** an [`AgentDef`] tree. Confirmed by reading `frameworks/serializer.py`
//! (`_serialize_skill`) and `runtime/runtime.py`'s `_deploy_via_server`: python's `skill()`
//! returns an `Agent` instance with `_framework = "skill"` and `_framework_config` set to the
//! exact dict this module calls `raw_config`; `runtime.compile()`/`deploy()`/`start()` detect
//! that marker and send `{"framework": "skill", "rawConfig": raw_config}` — precisely the shape
//! [`super::AgentRuntime::compile_framework`]/[`super::AgentRuntime::deploy_framework`]/
//! [`super::AgentRuntime::start_framework`]/[`super::AgentRuntime::run_framework`] (added for the
//! *other* framework family — LangChain/LangGraph/Claude-Agent-SDK/OpenAI-Agents adapters) already
//! send. [`load_skill`] therefore returns a [`SkillAgent`] carrying `raw_config` directly usable
//! with those same methods — no new wire-serialization path needed.
//!
//! Python also lets a skill `Agent` be nested as a *sub-agent* of an ordinary native agent
//! (`config_serializer.py`'s `_serialize_agent` special-cases `_framework == "skill"` inline
//! while recursing a tree, so `Agent(agents=[skill_agent, ...])` or `agent_tool(skill_agent)`
//! both work). [`SkillAgent::into_agent_def`] ports this: it builds an [`AgentDef`] carrying
//! the same `framework`/`framework_config` marker (see [`AgentDef::with_framework`]), so a
//! loaded skill can be passed to [`AgentDef::with_sub_agent`]/[`super::ToolDef::agent`] like
//! any other sub-agent.
//!
//! ## Worker registration
//!
//! [`create_skill_workers`] mirrors python's `create_skill_workers` + `frameworks/serializer.py`'s
//! `_serialize_skill`: one tool per discovered script (runs it as a subprocess, 300s timeout) plus
//! one `read_skill_file` tool if there are any allowed resource files. Register the returned
//! [`ToolDef`]s with [`super::AgentRuntime::serve_tools`].
//!
//! **Known upstream oddity, ported faithfully, not "fixed":** `_serialize_skill` builds the
//! *same* `{"command": <string>}` input schema for every skill worker, including
//! `read_skill_file` — whose actual parameter is `path`, not `command`. Since python's tool
//! dispatch (`run_tool_task`, `_dispatch.py`) maps a task's input fields to the target function's
//! *real* parameter names via introspection (not the declared schema), an LLM faithfully filling
//! in `command` per the schema would call `read_skill_file` with no `path` argument at all. This
//! module reproduces the same schema (for wire fidelity) and reads the value the handler actually
//! needs (`path`) directly, matching what the real python function parameter name is — i.e. the
//! same latent mismatch exists on both sides, ported byte-for-byte rather than silently patched.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::{ConductorError, Result};

use super::tool::ToolDef;

// Hardcoded, compile-time-valid patterns — the `unwrap()`s here can never actually fail.
#[allow(clippy::unwrap_used)]
static FRONTMATTER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n").unwrap());
#[allow(clippy::unwrap_used)]
static BODY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)^---\s*\n.*?\n---\s*\n(.*)").unwrap());
#[allow(clippy::unwrap_used)]
static CROSS_SKILL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:invoke|use|call)\s+(?:the\s+)?([a-z][a-z0-9-]*)\s+skill").unwrap()
});

/// Characters (~15K tokens) above which a `SKILL.md` body is auto-split into `##`-heading
/// sections, matching python's `SECTION_SPLIT_THRESHOLD`.
const SECTION_SPLIT_THRESHOLD: usize = 50_000;

/// Extract the `name` and default `params` from `SKILL.md`'s YAML frontmatter, matching python's
/// `parse_frontmatter`. Narrowed to just these two fields — the only ones any caller in this
/// module (or python's) ever reads back out of the parsed frontmatter dict.
#[derive(Debug)]
struct Frontmatter {
    name: String,
    default_params: Vec<(String, Value)>,
}

fn parse_frontmatter(content: &str) -> Result<Frontmatter> {
    let Some(caps) = FRONTMATTER_RE.captures(content) else {
        return Err(ConductorError::agent(
            "SKILL.md missing required 'name' field in frontmatter",
        ));
    };
    let yaml_text = &caps[1];
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml_text)
        .map_err(|e| ConductorError::agent(format!("invalid YAML frontmatter: {e}")))?;
    let mapping = doc
        .as_mapping()
        .ok_or_else(|| ConductorError::agent("SKILL.md frontmatter is not a YAML mapping"))?;

    let name = mapping
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ConductorError::agent("SKILL.md missing required 'name' field in frontmatter")
        })?
        .to_string();

    let mut default_params = Vec::new();
    if let Some(params_value) = mapping.get("params") {
        if let Some(params_mapping) = params_value.as_mapping() {
            for (k, v) in params_mapping {
                let Some(pname) = k.as_str() else { continue };
                let default_value = match v.as_mapping().and_then(|m| m.get("default")) {
                    Some(default) => yaml_to_json(default)?,
                    None => yaml_to_json(v)?,
                };
                default_params.push((pname.to_string(), default_value));
            }
        }
    }

    Ok(Frontmatter {
        name,
        default_params,
    })
}

fn yaml_to_json(value: &serde_yaml_ng::Value) -> Result<Value> {
    serde_json::to_value(value)
        .map_err(|e| ConductorError::agent(format!("invalid YAML value: {e}")))
}

/// Extract the markdown body after frontmatter, matching python's `extract_body`.
fn extract_body(content: &str) -> String {
    match BODY_RE.captures(content) {
        Some(caps) => caps[1].trim().to_string(),
        None => content.to_string(),
    }
}

/// Slugify a heading: lowercase, spaces to hyphens, strip special chars — matches python's
/// `slugify` exactly.
fn slugify(text: &str) -> String {
    let lowered = text.to_lowercase();
    let filtered: String = lowered
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || *c == '-')
        .collect();
    let mut slug = String::new();
    let mut last_was_dash = false;
    for c in filtered.trim().chars() {
        if c.is_whitespace() || c == '-' {
            if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        } else {
            slug.push(c);
            last_was_dash = false;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Split a `SKILL.md` body into sections by `##` headings, matching python's
/// `split_into_sections`. Returns ordered `(slug, section_text)` pairs (heading line included in
/// each section's text); content before the first `##` heading is dropped, matching python's
/// preamble skip.
fn split_into_sections(body: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in body.lines() {
        if line.starts_with("## ") {
            if let Some(heading) = current_heading.take() {
                let slug = slugify(&heading);
                if !slug.is_empty() {
                    result.push((slug, current_lines.join("\n").trim().to_string()));
                }
            }
            current_heading = line.strip_prefix("## ").map(|s| s.trim().to_string());
            current_lines = vec![line];
        } else if current_heading.is_some() {
            current_lines.push(line);
        }
    }
    if let Some(heading) = current_heading.take() {
        let slug = slugify(&heading);
        if !slug.is_empty() {
            result.push((slug, current_lines.join("\n").trim().to_string()));
        }
    }
    result
}

fn extension_language(ext: &str) -> Option<&'static str> {
    match ext {
        ".py" => Some("python"),
        ".sh" => Some("bash"),
        ".js" | ".mjs" | ".ts" => Some("node"),
        ".rb" => Some("ruby"),
        _ => None,
    }
}

/// Detect script language from file extension or shebang, matching python's `detect_language`.
fn detect_language(path: &Path) -> String {
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    if let Some(lang) = extension_language(&ext) {
        return lang.to_string();
    }
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Some(first_line) = content.split('\n').next() {
            if first_line.starts_with("#!") {
                for (key, lang) in [
                    ("python3", "python"),
                    ("python", "python"),
                    ("bash", "bash"),
                    ("sh", "bash"),
                    ("node", "node"),
                    ("ruby", "ruby"),
                ] {
                    if first_line.contains(key) {
                        return lang.to_string();
                    }
                }
            }
        }
    }
    "bash".to_string()
}

/// Format skill parameters as a prompt prefix, matching python's `format_skill_params`.
fn format_skill_params(params: &[(String, Value)]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}: {}", display_param_value(v)))
        .collect();
    format!("[Skill Parameters]\n{}", lines.join("\n"))
}

fn display_param_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Prepend skill parameters to the user prompt, matching python's `format_prompt_with_params`.
pub fn format_prompt_with_params(prompt: &str, params: &[(String, Value)]) -> String {
    let prefix = format_skill_params(params);
    if prefix.is_empty() {
        return prompt.to_string();
    }
    format!("{prefix}\n\n[User Request]\n{prompt}")
}

/// Merge *overrides* onto *defaults* with python dict-update ordering: an existing key keeps its
/// position but takes the override's value; a brand-new key is appended in override order.
fn merge_ordered_params(
    defaults: &[(String, Value)],
    overrides: &[(String, Value)],
) -> Vec<(String, Value)> {
    let mut result = defaults.to_vec();
    for (k, v) in overrides {
        if let Some(entry) = result.iter_mut().find(|(rk, _)| rk == k) {
            entry.1 = v.clone();
        } else {
            result.push((k.clone(), v.clone()));
        }
    }
    result
}

/// A discovered skill script, matching python's per-script entry.
#[derive(Debug, Clone)]
struct ScriptInfo {
    filename: String,
    language: String,
    path: PathBuf,
}

/// Options for [`load_skill`], matching python's `skill()` keyword-only optional params.
#[derive(Debug, Clone, Default)]
pub struct SkillOptions {
    /// Model for the orchestrator agent; also the default for sub-agents.
    pub model: Option<String>,
    /// Per-sub-agent model overrides.
    pub agent_models: HashMap<String, String>,
    /// Additional directories to search for cross-skill references.
    pub search_path: Vec<String>,
    /// Runtime parameter overrides, merged on top of the `SKILL.md` frontmatter's declared
    /// defaults.
    pub params: HashMap<String, Value>,
}

/// A loaded Agent Skill — python's `Agent` instance with `_framework = "skill"`. Not an
/// [`super::AgentDef`]; see the module doc for why and how to run one.
#[derive(Debug, Clone)]
pub struct SkillAgent {
    pub name: String,
    pub model: Option<String>,
    /// The exact dict python calls `_framework_config` — feed this straight to
    /// [`super::AgentRuntime::compile_framework`]/[`super::AgentRuntime::deploy_framework`]/
    /// [`super::AgentRuntime::start_framework`]/[`super::AgentRuntime::run_framework`] with
    /// `framework = "skill"`.
    pub raw_config: Value,
    skill_path: PathBuf,
    scripts: Vec<(String, ScriptInfo)>,
    sections: Vec<(String, String)>,
    resource_files: Vec<String>,
}

impl SkillAgent {
    /// Convert this loaded skill into an [`AgentDef`] carrying the `"skill"` framework marker
    /// (see [`AgentDef::with_framework`]) — the piece that lets a skill be nested as a
    /// sub-agent of an ordinary native agent tree (`agents=[...]`/[`super::ToolDef::agent`]),
    /// matching python's `config_serializer.py::_serialize_agent`'s inline `_framework == "skill"`
    /// recursion case. For the standalone (top-level) case, use [`SkillAgent::raw_config`]
    /// directly with `compile_framework`/`deploy_framework`/`start_framework`/`run_framework`
    /// instead — nesting isn't required for that path.
    pub fn into_agent_def(self) -> Result<super::def::AgentDef> {
        let mut agent =
            super::def::AgentDef::new(self.name)?.with_framework("skill", self.raw_config);
        if let Some(model) = self.model {
            agent = agent.with_model(model);
        }
        Ok(agent)
    }
}

fn interpreter_for_language(language: &str) -> &'static str {
    match language {
        "python" => "python3",
        "node" => "node",
        "ruby" => "ruby",
        _ => "bash",
    }
}

/// Run one skill script as a subprocess, matching python's `ScriptRunner.__call__` — including
/// its "never raise, always return a descriptive string" contract (errors surface as
/// `"ERROR..."`-prefixed *successful* tool output, not a failed task).
async fn run_skill_script(interpreter: &str, script_path: &Path, args: &Value) -> Value {
    let command = args.get("command").and_then(Value::as_str).unwrap_or("");
    let extra_args = if command.is_empty() {
        Vec::new()
    } else {
        match shell_words::split(command) {
            Ok(tokens) => tokens,
            Err(e) => return Value::String(format!("ERROR: {e}")),
        }
    };

    let mut cmd = Command::new(interpreter);
    cmd.arg(script_path).args(&extra_args).stdin(Stdio::null());

    let output = match timeout(Duration::from_secs(300), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Value::String(format!("ERROR: {e}")),
        Err(_) => return Value::String("ERROR: Script execution timed out (300s)".to_string()),
    };

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Value::String(format!("ERROR (exit {code}):\n{stderr}"));
    }
    Value::String(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Read one allowed skill resource (or virtual `skill_section:*` entry), matching python's
/// `SkillFileReader.__call__` — same "never raise" contract as [`run_skill_script`]. Reads the
/// `path` key directly (see the module doc's note on the schema/param-name mismatch this
/// reproduces from python).
fn read_skill_file(
    skill_dir: &Path,
    allowed: &HashSet<String>,
    sections: &[(String, String)],
    path_arg: &str,
) -> String {
    if !allowed.contains(path_arg) {
        let mut sorted: Vec<&String> = allowed.iter().collect();
        sorted.sort();
        let listed = sorted
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ");
        return format!("ERROR: '{path_arg}' not found. Available: [{listed}]");
    }

    if let Some(section_name) = path_arg.strip_prefix("skill_section:") {
        return match sections.iter().find(|(k, _)| k == section_name) {
            Some((_, content)) => content.clone(),
            None => format!("ERROR: section '{section_name}' not found"),
        };
    }

    let target = skill_dir.join(path_arg);
    let resolved_target = match std::fs::canonicalize(&target) {
        Ok(p) => p,
        Err(e) => return format!("ERROR reading '{path_arg}': {e}"),
    };
    let resolved_dir = match std::fs::canonicalize(skill_dir) {
        Ok(p) => p,
        Err(e) => return format!("ERROR reading '{path_arg}': {e}"),
    };
    if !resolved_target.starts_with(&resolved_dir) {
        return format!("ERROR: '{path_arg}' is outside the skill directory");
    }
    match std::fs::read_to_string(&target) {
        Ok(content) => content,
        Err(e) => format!("ERROR reading '{path_arg}': {e}"),
    }
}

/// The fixed input schema python's `_serialize_skill` gives every skill worker — see the module
/// doc for the `read_skill_file` naming mismatch this intentionally preserves.
fn skill_worker_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {"type": "string", "description": "Arguments to pass"},
        },
    })
}

/// Build the worker [`ToolDef`]s for a loaded skill, matching python's `create_skill_workers`
/// (+ `frameworks/serializer.py`'s `_serialize_skill`, which is what actually turns them into
/// registrable workers on the python side). Register the result with
/// [`super::AgentRuntime::serve_tools`].
pub fn create_skill_workers(agent: &SkillAgent) -> Vec<ToolDef> {
    let mut workers = Vec::new();

    for (tool_name, script) in &agent.scripts {
        let worker_name = format!("{}__{}", agent.name, tool_name);
        let description = format!("Run {tool_name} script from {} skill", agent.name);
        let interpreter = interpreter_for_language(&script.language).to_string();
        let script_path = script.path.clone();
        workers.push(ToolDef::function::<Value, _, _>(
            worker_name,
            description,
            skill_worker_input_schema(),
            move |args: Value| {
                let interpreter = interpreter.clone();
                let script_path = script_path.clone();
                async move { Ok(run_skill_script(&interpreter, &script_path, &args).await) }
            },
        ));
    }

    let allowed_files: HashSet<String> = agent.resource_files.iter().cloned().collect();
    if !allowed_files.is_empty() {
        let worker_name = format!("{}__read_skill_file", agent.name);
        let description = format!("Read resource files from {} skill", agent.name);
        let skill_dir = agent.skill_path.clone();
        let sections = agent.sections.clone();
        workers.push(ToolDef::function::<Value, _, _>(
            worker_name,
            description,
            skill_worker_input_schema(),
            move |args: Value| {
                let skill_dir = skill_dir.clone();
                let sections = sections.clone();
                let allowed_files = allowed_files.clone();
                async move {
                    let path_arg = args.get("path").and_then(Value::as_str).unwrap_or("");
                    Ok(Value::String(read_skill_file(
                        &skill_dir,
                        &allowed_files,
                        &sections,
                        path_arg,
                    )))
                }
            },
        ));
    }

    workers
}

fn read_agent_files(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with("-agent.md"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let agent_name = stem.strip_suffix("-agent").unwrap_or(&stem).to_string();
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ConductorError::agent(format!("failed to read {}: {e}", path.display()))
        })?;
        entries.push((agent_name, content));
    }
    Ok(entries)
}

fn discover_scripts(dir: &Path) -> Result<Vec<(String, ScriptInfo)>> {
    let scripts_dir = dir.join("scripts");
    if !scripts_dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&scripts_dir)
        .map_err(|e| {
            ConductorError::agent(format!("failed to read {}: {e}", scripts_dir.display()))
        })?
        .flatten()
    {
        let path = entry.path();
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let language = detect_language(&path);
            (
                stem,
                ScriptInfo {
                    filename,
                    language,
                    path,
                },
            )
        })
        .collect())
}

fn discover_resource_files(dir: &Path) -> Vec<String> {
    let mut resource_files = Vec::new();
    for subdir in ["references", "examples", "assets"] {
        let sub_path = dir.join(subdir);
        if sub_path.exists() {
            let mut files: Vec<String> = walk_files(&sub_path)
                .into_iter()
                .filter_map(|f| {
                    f.strip_prefix(dir)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .collect();
            files.sort();
            resource_files.extend(files);
        }
    }
    let mut root_files: Vec<String> = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if path.is_file()
                && name != "SKILL.md"
                && !name.ends_with("-agent.md")
                && name != "skill.yaml"
                && name != "skill.toml"
            {
                root_files.push(name.to_string());
            }
        }
    }
    root_files.sort();
    resource_files.extend(root_files);
    resource_files
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(read_dir) = std::fs::read_dir(&current) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() {
                    files.push(path);
                }
            }
        }
    }
    files
}

/// Resolve cross-skill references found in `SKILL.md`'s body, matching python's
/// `resolve_cross_skills`. Scans for patterns like `"invoke writing-plans skill"` and resolves
/// them from `search_path` plus the standard sibling/`.agents/skills` locations.
fn resolve_cross_skills(
    skill_md: &str,
    skill_path: &Path,
    search_path: &[String],
    seen: &HashSet<PathBuf>,
) -> Result<Value> {
    let body = extract_body(skill_md);
    let matches: HashSet<String> = CROSS_SKILL_RE
        .captures_iter(&body)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
        .collect();
    if matches.is_empty() {
        return Ok(json!({}));
    }

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent) = skill_path.parent() {
        if parent.exists() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".agents").join("skills"));
    }
    if let Some(home) = dirs_home() {
        dirs.push(home.join(".agents").join("skills"));
    }
    for p in search_path {
        dirs.push(expand_and_resolve(p));
    }

    let mut seen = seen.clone();
    let resolved_skill_path = skill_path
        .canonicalize()
        .unwrap_or_else(|_| skill_path.to_path_buf());
    seen.insert(resolved_skill_path.clone());

    let mut cross_refs = serde_json::Map::new();
    for ref_name in &matches {
        for d in &dirs {
            let ref_dir = d.join(ref_name);
            let ref_dir_resolved = ref_dir.canonicalize().unwrap_or_else(|_| ref_dir.clone());
            if !ref_dir.join("SKILL.md").exists() || ref_dir_resolved == resolved_skill_path {
                continue;
            }
            if seen.contains(&ref_dir_resolved) {
                return Err(ConductorError::agent(format!(
                    "Circular skill reference detected: {ref_name}"
                )));
            }
            let ref_md = std::fs::read_to_string(ref_dir.join("SKILL.md")).map_err(|e| {
                ConductorError::agent(format!("failed to read cross-skill SKILL.md: {e}"))
            })?;
            let ref_frontmatter = parse_frontmatter(&ref_md)?;
            let ref_agent_files = read_agent_files(&ref_dir)?;
            let ref_scripts = discover_scripts(&ref_dir)?;
            let ref_resources = discover_resource_files(&ref_dir);
            let ref_body = extract_body(&ref_md);
            let ref_sections = if ref_body.len() > SECTION_SPLIT_THRESHOLD {
                split_into_sections(&ref_body)
            } else {
                Vec::new()
            };
            let mut ref_resources_with_sections = ref_resources;
            for (section_name, _) in &ref_sections {
                ref_resources_with_sections.push(format!("skill_section:{section_name}"));
            }

            let mut nested_seen = seen.clone();
            nested_seen.insert(ref_dir_resolved.clone());
            let nested_refs =
                resolve_cross_skills(&ref_md, &ref_dir_resolved, search_path, &nested_seen)?;

            let default_params_value: serde_json::Map<String, Value> =
                ref_frontmatter.default_params.iter().cloned().collect();

            cross_refs.insert(
                ref_name.clone(),
                json!({
                    "skillMd": ref_md,
                    "agentFiles": ref_agent_files
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect::<serde_json::Map<String, Value>>(),
                    "scripts": ref_scripts
                        .iter()
                        .map(|(k, v)| (k.clone(), json!({"filename": v.filename, "language": v.language})))
                        .collect::<serde_json::Map<String, Value>>(),
                    "resourceFiles": ref_resources_with_sections,
                    "crossSkillRefs": nested_refs,
                    "defaultParams": default_params_value.clone(),
                    "params": default_params_value,
                    "skillSections": ref_sections
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect::<serde_json::Map<String, Value>>(),
                }),
            );
            break;
        }
    }
    Ok(Value::Object(cross_refs))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn expand_and_resolve(p: &str) -> PathBuf {
    let expanded = if p.starts_with('~') {
        match dirs_home() {
            Some(home) => home.join(p.trim_start_matches('~').trim_start_matches('/')),
            None => PathBuf::from(p),
        }
    } else {
        PathBuf::from(p)
    };
    expanded.canonicalize().unwrap_or(expanded)
}

/// Load an Agent Skills directory as a [`SkillAgent`], matching python's `skill()`. See the
/// module doc for the wire mechanism (a "framework" marker, not an [`super::AgentDef`]) and what
/// is and isn't ported.
pub fn load_skill(path: impl AsRef<Path>, options: SkillOptions) -> Result<SkillAgent> {
    let path = expand_and_resolve(&path.as_ref().to_string_lossy());

    let skill_md_path = path.join("SKILL.md");
    if !skill_md_path.exists() {
        return Err(ConductorError::agent(format!(
            "Directory {} is not a valid skill: SKILL.md not found",
            path.display()
        )));
    }
    let mut skill_md = std::fs::read_to_string(&skill_md_path).map_err(|e| {
        ConductorError::agent(format!("failed to read {}: {e}", skill_md_path.display()))
    })?;
    let frontmatter = parse_frontmatter(&skill_md)?;

    let override_params: Vec<(String, Value)> = options.params.into_iter().collect();
    let merged_params = merge_ordered_params(&frontmatter.default_params, &override_params);

    let agent_files = read_agent_files(&path)?;
    let scripts = discover_scripts(&path)?;
    let mut resource_files = discover_resource_files(&path);

    let cross_refs = resolve_cross_skills(&skill_md, &path, &options.search_path, &HashSet::new())?;

    let body = extract_body(&skill_md);
    let mut sections = Vec::new();
    if body.len() > SECTION_SPLIT_THRESHOLD {
        sections = split_into_sections(&body);
        for (section_name, _) in &sections {
            resource_files.push(format!("skill_section:{section_name}"));
        }
    }

    if !merged_params.is_empty() {
        let param_block = format_skill_params(&merged_params);
        skill_md = format!("{skill_md}\n\n{param_block}\n");
    }

    let model_str = options.model.clone().unwrap_or_default();
    let raw_config = json!({
        "model": model_str,
        "agentModels": options.agent_models,
        "skillMd": skill_md,
        "agentFiles": agent_files
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<serde_json::Map<String, Value>>(),
        "scripts": scripts
            .iter()
            .map(|(k, v)| (k.clone(), json!({"filename": v.filename, "language": v.language})))
            .collect::<serde_json::Map<String, Value>>(),
        "resourceFiles": resource_files,
        "crossSkillRefs": cross_refs,
        "defaultParams": frontmatter.default_params.iter().cloned().collect::<serde_json::Map<String, Value>>(),
        "params": merged_params.iter().cloned().collect::<serde_json::Map<String, Value>>(),
    });

    Ok(SkillAgent {
        name: frontmatter.name,
        model: options.model.filter(|m| !m.is_empty()),
        raw_config,
        skill_path: path,
        scripts,
        sections,
        resource_files,
    })
}

/// Load all skills from a directory (each immediate subdirectory containing a `SKILL.md`),
/// matching python's `load_skills`.
pub fn load_skills(
    path: impl AsRef<Path>,
    model: Option<&str>,
    agent_models: &HashMap<String, HashMap<String, String>>,
) -> Result<HashMap<String, SkillAgent>> {
    let path = expand_and_resolve(&path.as_ref().to_string_lossy());
    let mut skills = HashMap::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&path)
        .map_err(|e| ConductorError::agent(format!("failed to read {}: {e}", path.display())))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("SKILL.md").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let overrides = agent_models.get(&name).cloned().unwrap_or_default();
        let options = SkillOptions {
            model: model.map(String::from),
            agent_models: overrides,
            search_path: Vec::new(),
            params: HashMap::new(),
        };
        skills.insert(name, load_skill(&dir, options)?);
    }
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn temp_skill_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rust_sdk_skill_test_{name}_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("  multiple   spaces  "), "multiple-spaces");
        assert_eq!(slugify("---already-dashed---"), "already-dashed");
        assert_eq!(slugify("###"), "");
    }

    #[test]
    fn test_extract_body_with_frontmatter() {
        let content = "---\nname: test\n---\n\nBody text here.";
        assert_eq!(extract_body(content), "Body text here.");
    }

    #[test]
    fn test_extract_body_without_frontmatter() {
        let content = "No frontmatter here.";
        assert_eq!(extract_body(content), "No frontmatter here.");
    }

    #[test]
    fn test_parse_frontmatter_requires_name() {
        let err = parse_frontmatter("---\nfoo: bar\n---\nbody").unwrap_err();
        assert!(err.to_string().contains("missing required 'name'"));
    }

    #[test]
    fn test_parse_frontmatter_extracts_name() {
        let fm = parse_frontmatter("---\nname: my-skill\n---\nbody").unwrap();
        assert_eq!(fm.name, "my-skill");
        assert!(fm.default_params.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_extracts_params_with_defaults() {
        let yaml =
            "---\nname: my-skill\nparams:\n  rounds:\n    default: 3\n  verbose: true\n---\nbody";
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.name, "my-skill");
        let map: HashMap<String, Value> = fm.default_params.into_iter().collect();
        assert_eq!(map.get("rounds"), Some(&json!(3)));
        assert_eq!(map.get("verbose"), Some(&json!(true)));
    }

    #[test]
    fn test_split_into_sections_skips_preamble_and_slugifies() {
        let body =
            "Preamble text.\n\n## First Section\ncontent a\n\n## Second Section!\ncontent b\n";
        let sections = split_into_sections(body);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, "first-section");
        assert!(sections[0].1.starts_with("## First Section"));
        assert_eq!(sections[1].0, "second-section");
    }

    #[test]
    fn test_format_skill_params_empty() {
        assert_eq!(format_skill_params(&[]), "");
    }

    #[test]
    fn test_format_skill_params_lists_keys() {
        let params = vec![
            ("rounds".to_string(), json!(3)),
            ("verbose".to_string(), json!(true)),
        ];
        assert_eq!(
            format_skill_params(&params),
            "[Skill Parameters]\nrounds: 3\nverbose: true"
        );
    }

    #[test]
    fn test_format_prompt_with_params_prepends_when_nonempty() {
        let params = vec![("rounds".to_string(), json!(3))];
        let result = format_prompt_with_params("Do the thing.", &params);
        assert_eq!(
            result,
            "[Skill Parameters]\nrounds: 3\n\n[User Request]\nDo the thing."
        );
    }

    #[test]
    fn test_format_prompt_with_params_unchanged_when_empty() {
        assert_eq!(
            format_prompt_with_params("Do the thing.", &[]),
            "Do the thing."
        );
    }

    #[test]
    fn test_merge_ordered_params_keeps_default_position_appends_new() {
        let defaults = vec![("a".to_string(), json!(1)), ("b".to_string(), json!(2))];
        let overrides = vec![("b".to_string(), json!(20)), ("c".to_string(), json!(3))];
        let merged = merge_ordered_params(&defaults, &overrides);
        assert_eq!(
            merged,
            vec![
                ("a".to_string(), json!(1)),
                ("b".to_string(), json!(20)),
                ("c".to_string(), json!(3)),
            ]
        );
    }

    #[test]
    fn test_detect_language_by_extension() {
        let dir = temp_skill_dir("detect_ext");
        let path = dir.join("run.py");
        write_file(&path, "print(1)");
        assert_eq!(detect_language(&path), "python");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_detect_language_by_shebang_when_no_known_extension() {
        let dir = temp_skill_dir("detect_shebang");
        let path = dir.join("run");
        write_file(&path, "#!/usr/bin/env python3\nprint(1)\n");
        assert_eq!(detect_language(&path), "python");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_detect_language_defaults_to_bash() {
        let dir = temp_skill_dir("detect_default");
        let path = dir.join("run.unknownext");
        write_file(&path, "echo hi\n");
        assert_eq!(detect_language(&path), "bash");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skill_requires_skill_md() {
        let dir = temp_skill_dir("no_skill_md");
        let err = load_skill(&dir, SkillOptions::default()).unwrap_err();
        assert!(err.to_string().contains("SKILL.md not found"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skill_basic() {
        let dir = temp_skill_dir("basic");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: basic-skill\n---\n\nDo the thing.",
        );
        let agent = load_skill(&dir, SkillOptions::default()).unwrap();
        assert_eq!(agent.name, "basic-skill");
        assert_eq!(
            agent.raw_config["skillMd"],
            json!("---\nname: basic-skill\n---\n\nDo the thing.")
        );
        assert_eq!(agent.raw_config["model"], json!(""));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_skill_agent_into_agent_def_carries_framework_marker() {
        let dir = temp_skill_dir("into_agent_def");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: nested-skill\n---\n\nBody.",
        );
        let skill_agent = load_skill(
            &dir,
            SkillOptions {
                model: Some("openai/gpt-4o".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let raw_config = skill_agent.raw_config.clone();

        let agent_def = skill_agent.into_agent_def().unwrap();
        assert_eq!(agent_def.name, "nested-skill");
        assert_eq!(agent_def.model, Some("openai/gpt-4o".to_string()));
        assert_eq!(agent_def.framework, Some("skill".to_string()));
        assert_eq!(agent_def.framework_config, Some(raw_config));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_skill_agent_can_be_nested_as_a_sub_agent() {
        let dir = temp_skill_dir("nested_sub_agent");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: nested-skill\n---\n\nBody.",
        );
        let skill_agent = load_skill(&dir, SkillOptions::default()).unwrap();

        let parent = super::super::def::AgentDef::new("parent")
            .unwrap()
            .with_sub_agent(skill_agent.into_agent_def().unwrap())
            .unwrap();
        assert_eq!(parent.agents.len(), 1);
        assert_eq!(parent.agents[0].framework, Some("skill".to_string()));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skill_discovers_agent_files() {
        let dir = temp_skill_dir("agent_files");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: multi-agent-skill\n---\n\nBody.",
        );
        write_file(&dir.join("researcher-agent.md"), "You research things.");
        let agent = load_skill(&dir, SkillOptions::default()).unwrap();
        assert_eq!(
            agent.raw_config["agentFiles"]["researcher"],
            json!("You research things.")
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skill_discovers_scripts_and_resources() {
        let dir = temp_skill_dir("scripts_resources");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: scripty-skill\n---\n\nBody.",
        );
        write_file(&dir.join("scripts").join("run.py"), "print('hi')");
        write_file(&dir.join("references").join("notes.md"), "notes");

        let agent = load_skill(&dir, SkillOptions::default()).unwrap();
        assert_eq!(
            agent.raw_config["scripts"]["run"],
            json!({"filename": "run.py", "language": "python"})
        );
        let resource_files: Vec<String> = agent.raw_config["resourceFiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(resource_files.contains(&"references/notes.md".to_string()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skill_merges_params_and_injects_param_block() {
        let dir = temp_skill_dir("params");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: params-skill\nparams:\n  rounds:\n    default: 1\n---\n\nBody.",
        );
        let mut overrides = HashMap::new();
        overrides.insert("rounds".to_string(), json!(5));
        let agent = load_skill(
            &dir,
            SkillOptions {
                params: overrides,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(agent.raw_config["params"]["rounds"], json!(5));
        assert_eq!(agent.raw_config["defaultParams"]["rounds"], json!(1));
        assert!(agent.raw_config["skillMd"]
            .as_str()
            .unwrap()
            .contains("[Skill Parameters]\nrounds: 5"));
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_run_skill_script_success() {
        let dir = temp_skill_dir("run_script_success");
        let script_path = dir.join("run.py");
        write_file(&script_path, "print('hello from script')");
        let result = run_skill_script("python3", &script_path, &json!({})).await;
        assert_eq!(result, json!("hello from script\n"));
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_run_skill_script_nonzero_exit_is_error_string_not_err() {
        let dir = temp_skill_dir("run_script_error");
        let script_path = dir.join("run.py");
        write_file(&script_path, "import sys\nsys.exit(3)");
        let result = run_skill_script("python3", &script_path, &json!({})).await;
        assert!(result.as_str().unwrap().starts_with("ERROR (exit 3):"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_skill_file_rejects_non_allowed_path() {
        let dir = temp_skill_dir("read_file_reject");
        let allowed = HashSet::new();
        let result = read_skill_file(&dir, &allowed, &[], "secret.txt");
        assert!(result.starts_with("ERROR: 'secret.txt' not found."));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_skill_file_reads_allowed_file() {
        let dir = temp_skill_dir("read_file_ok");
        write_file(&dir.join("notes.txt"), "hello notes");
        let mut allowed = HashSet::new();
        allowed.insert("notes.txt".to_string());
        let result = read_skill_file(&dir, &allowed, &[], "notes.txt");
        assert_eq!(result, "hello notes");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_skill_file_rejects_path_traversal() {
        let dir = temp_skill_dir("read_file_traversal");
        let outside = temp_skill_dir("read_file_traversal_outside");
        write_file(&outside.join("secret.txt"), "top secret");
        let mut allowed = HashSet::new();
        let traversal = format!(
            "../{}/secret.txt",
            outside.file_name().unwrap().to_str().unwrap()
        );
        allowed.insert(traversal.clone());
        let result = read_skill_file(&dir, &allowed, &[], &traversal);
        assert!(result.contains("outside the skill directory"));
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn test_read_skill_file_serves_virtual_section() {
        let dir = temp_skill_dir("read_file_section");
        let sections = vec![("intro".to_string(), "## Intro\nhello".to_string())];
        let mut allowed = HashSet::new();
        allowed.insert("skill_section:intro".to_string());
        let result = read_skill_file(&dir, &allowed, &sections, "skill_section:intro");
        assert_eq!(result, "## Intro\nhello");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_create_skill_workers_names_and_count() {
        let dir = temp_skill_dir("workers");
        write_file(
            &dir.join("SKILL.md"),
            "---\nname: worker-skill\n---\n\nBody.",
        );
        write_file(&dir.join("scripts").join("run.py"), "print('hi')");
        write_file(&dir.join("references").join("notes.md"), "notes");
        let agent = load_skill(&dir, SkillOptions::default()).unwrap();
        let workers = create_skill_workers(&agent);
        let names: Vec<&str> = workers.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"worker-skill__run"));
        assert!(names.contains(&"worker-skill__read_skill_file"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_skills_loads_all_subdirectories() {
        let root = temp_skill_dir("load_skills_root");
        write_file(
            &root.join("skill-a").join("SKILL.md"),
            "---\nname: skill-a\n---\n\nBody.",
        );
        write_file(
            &root.join("skill-b").join("SKILL.md"),
            "---\nname: skill-b\n---\n\nBody.",
        );
        let skills = load_skills(&root, None, &HashMap::new()).unwrap();
        assert_eq!(skills.len(), 2);
        assert!(skills.contains_key("skill-a"));
        assert!(skills.contains_key("skill-b"));
        fs::remove_dir_all(&root).ok();
    }
}
