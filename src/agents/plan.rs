// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Typed plan builders for `Strategy::PlanExecute`.
//!
//! Ports python-sdk's `conductor.ai.agents.plans` (`plans.py`) field-for-field. These types
//! produce the JSON shape PAC (the server's `PLAN_AND_COMPILE` task) consumes — construct a
//! [`Plan`] in Rust instead of hand-building the JSON, then hand its [`Plan::to_value`] to
//! [`AgentRuntime::start`](super::runtime::AgentRuntime::start) as the `static_plan` field to
//! skip the planner LLM and run a fully deterministic pipeline (`runtime.run(harness,
//! plan=plan)` in python).
//!
//! ## `Ref` — simpler here than in python, same wire shape
//!
//! Python's `_serialize_value` recursively walks arbitrary `dict`/`list`/`tuple` trees looking
//! for embedded [`Ref`] markers to replace with their `{"$ref": "<step_id>"}` wire form, because
//! python's `Any`-typed `args`/`context` fields can hold a `Ref` at any nesting depth. Rust's
//! `args`/`context` fields are plain [`serde_json::Value`] instead — no parallel tree-walking
//! type is needed, because [`Ref`] converts directly to a [`Value`] (`Ref::to_value`, and
//! `From<Ref> for Value`), so a caller embeds one anywhere a `Value` is expected using ordinary
//! `serde_json::json!` nesting: `json!({"document": my_ref.to_value()})`. Same wire output as
//! python's walker produces, without needing to reproduce the walk.
//!
//! ## One place this is a Rust-native improvement, not just a port
//!
//! Python's `Op`/`Generate` mutual exclusivity (`args` XOR `generate`) is a runtime
//! `__post_init__` check (`ValueError` if both or neither are set). [`Op`] models this as an
//! enum ([`OpBody::Args`]/[`OpBody::Generate`]) instead, so the invalid state is unrepresentable
//! rather than merely rejected — [`Op::with_args`]/[`Op::with_generate`] are the only
//! constructors, and each fully determines `body`.

use serde_json::{Map, Value};

use crate::error::Result;

use super::def::AgentDef;
use super::tool::ToolDef;

/// A reference to a prior step's whole output.
///
/// Use anywhere a literal value would go in an [`Op`]'s args or a [`Generate`]'s context to
/// wire one step's output into another step's input — no JSON path, no field selection, the
/// whole result map becomes the value at that position. The referenced step must be declared in
/// this step's `depends_on` and must exist in the plan; the server rejects the plan at compile
/// time otherwise (no silent broken refs) — this type does not re-validate that here, matching
/// python (which also defers that check to the server).
///
/// For a parallel step, `Ref("a")` is the array of that step's branch results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub step_id: String,
}

impl Ref {
    pub fn new(step_id: impl Into<String>) -> Self {
        Self {
            step_id: step_id.into(),
        }
    }

    /// Wire form the server's PAC consumes: `{"$ref": "<step_id>"}`.
    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::json!({"$ref": self.step_id})
    }
}

impl From<Ref> for Value {
    fn from(r: Ref) -> Value {
        r.to_value()
    }
}

/// A reference document made available to the `PLAN_EXECUTE` planner.
///
/// Appended to the planner's user prompt as a `## Reference Context` block on every planner
/// invocation. Use to ground the planner in domain-specific rules the static `instructions`
/// string can't capture.
///
/// Exactly one of `text`/`url` is ever set — enforced by construction ([`Context::text`]/
/// [`Context::url`] are the only constructors), unlike python's runtime `__post_init__` check.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    text: Option<String>,
    url: Option<String>,
    headers: std::collections::HashMap<String, String>,
    required: bool,
    max_bytes: u32,
}

impl Context {
    /// Inline reference text — best for short, stable rules.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            url: None,
            headers: std::collections::HashMap::new(),
            required: true,
            max_bytes: 0x4000,
        }
    }

    /// HTTP GET on every planner run — no compile-time fetch, no cache, so doc edits go live
    /// without recompile.
    pub fn url(url: impl Into<String>) -> Self {
        Self {
            text: None,
            url: Some(url.into()),
            headers: std::collections::HashMap::new(),
            required: true,
            max_bytes: 0x4000,
        }
    }

    /// HTTP headers for a `url` context. May contain `${CRED_NAME}` placeholders that resolve
    /// against the agent's credential store at request time — same auth pipeline as an HTTP
    /// tool's headers. Ignored for a `text` context (matches python: only serialized when `url`
    /// is set).
    #[must_use]
    pub fn with_headers(mut self, headers: std::collections::HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// When `false`, a fetch failure substitutes a `[doc unavailable]` marker instead of
    /// failing the workflow. Default `true`. Ignored for a `text` context.
    #[must_use]
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Per-doc truncation cap. Default `16384`. Ignored for a `text` context.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: u32) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Wire form, matching python's `Context.to_dict` exactly (including that `headers`/
    /// `required`/`maxBytes` are only emitted for a `url` context, and `maxBytes` only when it
    /// differs from the default).
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        if let Some(text) = &self.text {
            map.insert("text".to_owned(), Value::String(text.clone()));
        }
        if let Some(url) = &self.url {
            map.insert("url".to_owned(), Value::String(url.clone()));
            if !self.headers.is_empty() {
                let headers = self
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect();
                map.insert("headers".to_owned(), Value::Object(headers));
            }
            if !self.required {
                map.insert("required".to_owned(), Value::Bool(false));
            }
            if self.max_bytes != 0x4000 {
                map.insert("maxBytes".to_owned(), Value::from(self.max_bytes));
            }
        }
        Value::Object(map)
    }
}

/// LLM-generated arguments for a tool call inside a plan step.
///
/// When an [`Op`] carries a `generate` body, the server emits an LLM call at run time that
/// produces the tool's args from these instructions, then runs the tool with the generated
/// args. Use this when arg values aren't known at plan-construction time.
#[derive(Debug, Clone, PartialEq)]
pub struct Generate {
    pub instructions: String,
    pub output_schema: String,
    pub max_tokens: Option<u32>,
    /// Extra text appended to the LLM's user message. A plain string, or a [`Ref`]'s
    /// [`Ref::to_value`] — when a ref, the server substitutes the upstream step's output at run
    /// time so the LLM sees real values instead of the literal `{"$ref":...}` marker.
    pub context: Option<Value>,
}

impl Generate {
    pub fn new(instructions: impl Into<String>, output_schema: impl Into<String>) -> Self {
        Self {
            instructions: instructions.into(),
            output_schema: output_schema.into(),
            max_tokens: None,
            context: None,
        }
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<Value>) -> Self {
        self.context = Some(context.into());
        self
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "instructions".to_owned(),
            Value::String(self.instructions.clone()),
        );
        map.insert(
            "output_schema".to_owned(),
            Value::String(self.output_schema.clone()),
        );
        if let Some(max_tokens) = self.max_tokens {
            map.insert("max_tokens".to_owned(), Value::from(max_tokens));
        }
        if let Some(context) = &self.context {
            map.insert("context".to_owned(), context.clone());
        }
        Value::Object(map)
    }
}

/// [`Op`]'s body — exactly one of a literal arg map or LLM-generated args. See the module doc
/// for why this is an enum rather than python's runtime-checked `Optional` pair.
#[derive(Debug, Clone, PartialEq)]
pub enum OpBody {
    /// Literal arg map for a deterministic call. May embed [`Ref`] values anywhere via
    /// [`Ref::to_value`].
    Args(Value),
    /// LLM-generated args (mutually exclusive with [`OpBody::Args`]).
    Generate(Generate),
}

/// A single tool invocation within a plan [`Step`].
#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub tool: String,
    pub body: OpBody,
}

impl Op {
    /// A deterministic call: `tool` runs with the literal `args` map.
    pub fn with_args(tool: impl Into<String>, args: Value) -> Self {
        Self {
            tool: tool.into(),
            body: OpBody::Args(args),
        }
    }

    /// A deferred call: `tool`'s args are produced by an LLM call at run time per `generate`.
    pub fn with_generate(tool: impl Into<String>, generate: Generate) -> Self {
        Self {
            tool: tool.into(),
            body: OpBody::Generate(generate),
        }
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("tool".to_owned(), Value::String(self.tool.clone()));
        match &self.body {
            OpBody::Args(args) => {
                map.insert("args".to_owned(), args.clone());
            }
            OpBody::Generate(generate) => {
                map.insert("generate".to_owned(), generate.to_value());
            }
        }
        Value::Object(map)
    }
}

/// A node in the plan DAG.
///
/// Steps run sequentially by default; `depends_on` overrides to express cross-step concurrency
/// (a step starts when all listed deps complete). `parallel = true` runs the step's own
/// `operations` concurrently (`FORK_JOIN`); without it, operations run in order within the step.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub id: String,
    pub operations: Vec<Op>,
    pub depends_on: Vec<String>,
    pub parallel: bool,
}

impl Step {
    pub fn new(id: impl Into<String>, operations: Vec<Op>) -> Self {
        Self {
            id: id.into(),
            operations,
            depends_on: Vec::new(),
            parallel: false,
        }
    }

    #[must_use]
    pub fn with_depends_on(
        mut self,
        depends_on: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.depends_on = depends_on.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_owned(), Value::String(self.id.clone()));
        map.insert(
            "operations".to_owned(),
            Value::Array(self.operations.iter().map(Op::to_value).collect()),
        );
        if !self.depends_on.is_empty() {
            map.insert(
                "depends_on".to_owned(),
                Value::Array(self.depends_on.iter().cloned().map(Value::String).collect()),
            );
        }
        if self.parallel {
            map.insert("parallel".to_owned(), Value::Bool(true));
        }
        Value::Object(map)
    }
}

/// A post-execution check, run after all [`Plan::steps`] complete. PAC routes the workflow to
/// [`Plan::on_success`] when every validation passes, else to [`Plan::on_failure`].
#[derive(Debug, Clone, PartialEq)]
pub struct Validation {
    pub tool: String,
    pub args: Option<Value>,
    /// Optional JS expression evaluated against the tool's output (`$` is the parsed output
    /// map). Returns truthy on pass. When omitted, PAC checks that `output.passed` is not
    /// `false` and that the output is not an `ERROR` string.
    pub success_condition: Option<String>,
}

impl Validation {
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args: None,
            success_condition: None,
        }
    }

    #[must_use]
    pub fn with_args(mut self, args: Value) -> Self {
        self.args = Some(args);
        self
    }

    #[must_use]
    pub fn with_success_condition(mut self, expr: impl Into<String>) -> Self {
        self.success_condition = Some(expr.into());
        self
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("tool".to_owned(), Value::String(self.tool.clone()));
        if let Some(args) = &self.args {
            map.insert("args".to_owned(), args.clone());
        }
        if let Some(expr) = &self.success_condition {
            map.insert("success_condition".to_owned(), Value::String(expr.clone()));
        }
        Value::Object(map)
    }
}

/// A tool call attached to [`Plan::on_success`]/[`Plan::on_failure`]. Same shape as a
/// deterministic [`Op`] (args only — no `generate`, since success/failure handlers run with
/// known context).
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub tool: String,
    pub args: Option<Value>,
}

impl Action {
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args: None,
        }
    }

    #[must_use]
    pub fn with_args(mut self, args: Value) -> Self {
        self.args = Some(args);
        self
    }

    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("tool".to_owned(), Value::String(self.tool.clone()));
        if let Some(args) = &self.args {
            map.insert("args".to_owned(), args.clone());
        }
        Value::Object(map)
    }
}

/// A compiled plan ready for `Strategy::PlanExecute` execution.
///
/// Construct in Rust and pass [`Plan::to_value`] as the `static_plan` field to
/// [`AgentRuntime::start`](super::runtime::AgentRuntime::start) to skip the planner LLM and run
/// a fully deterministic pipeline (matches python's `runtime.run(harness, plan=plan)`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub validation: Vec<Validation>,
    pub on_success: Vec<Action>,
    pub on_failure: Vec<Action>,
}

impl Plan {
    #[must_use]
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps,
            validation: Vec::new(),
            on_success: Vec::new(),
            on_failure: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_validation(mut self, validation: Vec<Validation>) -> Self {
        self.validation = validation;
        self
    }

    #[must_use]
    pub fn with_on_success(mut self, on_success: Vec<Action>) -> Self {
        self.on_success = on_success;
        self
    }

    #[must_use]
    pub fn with_on_failure(mut self, on_failure: Vec<Action>) -> Self {
        self.on_failure = on_failure;
        self
    }

    /// Wire form the server's `static_plan` field consumes, matching python's `Plan.to_dict`
    /// exactly (including which empty collections are omitted vs. always present).
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert(
            "steps".to_owned(),
            Value::Array(self.steps.iter().map(Step::to_value).collect()),
        );
        if !self.validation.is_empty() {
            map.insert(
                "validation".to_owned(),
                Value::Array(self.validation.iter().map(Validation::to_value).collect()),
            );
        }
        if !self.on_success.is_empty() {
            map.insert(
                "on_success".to_owned(),
                Value::Array(self.on_success.iter().map(Action::to_value).collect()),
            );
        }
        if !self.on_failure.is_empty() {
            map.insert(
                "on_failure".to_owned(),
                Value::Array(self.on_failure.iter().map(Action::to_value).collect()),
            );
        }
        Value::Object(map)
    }
}

/// Options for [`plan_execute`], beyond the required `name`/`tools`. Mirrors the keyword-only
/// parameters on python's `plan_execute()` function.
#[derive(Debug, Clone, Default)]
pub struct PlanExecuteOptions {
    /// Domain-level guidance for the planner. The server auto-appends a `## Available tools`
    /// block and a `## Plan schema` block; don't repeat them here. Leave empty (the default)
    /// when the caller always supplies a [`Plan`] directly via `static_plan` — the planner LLM
    /// still runs but its output is discarded by PAC's `extract_json`.
    pub planner_instructions: String,
    /// When `Some`, builds a fallback agent (with the same `tools` set) invoked when the plan
    /// fails mid-execution. `None` leaves the harness without a fallback (failures TERMINATE).
    pub fallback_instructions: Option<String>,
    /// LLM model string, applied to the planner, the fallback (if any), and the parent. When
    /// omitted, each sub-agent's own default applies.
    pub model: Option<String>,
    /// Turn cap applied to the fallback once it's invoked.
    pub fallback_max_turns: Option<u32>,
    /// Reference text for the planner prompt (see [`AgentDef::with_planner_contexts`]).
    pub planner_context: Vec<String>,
}

/// Construct a `Strategy::PlanExecute` harness in one call — wraps the boilerplate of building
/// a planner sub-agent, an optional fallback sub-agent, and the parent coordinator. Matches
/// python's `plan_execute()` function exactly, including its sub-agent naming convention
/// (`{name}_planner`, `{name}_fallback`).
///
/// # Errors
///
/// Returns [`crate::error::ConductorError::Agent`] if `name` (or the derived `{name}_planner`/`{name}_fallback` names) is empty or invalid -- see [`AgentDef::new`].
pub fn plan_execute(
    name: impl Into<String>,
    tools: Vec<ToolDef>,
    options: PlanExecuteOptions,
) -> Result<AgentDef> {
    let name = name.into();

    let mut planner =
        AgentDef::new(format!("{name}_planner"))?.with_instructions(options.planner_instructions);
    if let Some(model) = &options.model {
        planner = planner.with_model(model.clone());
    }

    let fallback = match &options.fallback_instructions {
        Some(fallback_instructions) => {
            let mut fallback = AgentDef::new(format!("{name}_fallback"))?
                .with_instructions(fallback_instructions.clone())
                .with_tools(tools.clone());
            if let Some(model) = &options.model {
                fallback = fallback.with_model(model.clone());
            }
            Some(fallback)
        }
        None => None,
    };

    let mut harness = AgentDef::new(name)?.with_planner(planner).with_tools(tools);
    if let Some(fallback) = fallback {
        harness = harness.with_fallback(fallback);
    }
    if let Some(model) = &options.model {
        harness = harness.with_model(model.clone());
    }
    if let Some(fallback_max_turns) = options.fallback_max_turns {
        harness = harness.with_fallback_max_turns(fallback_max_turns);
    }
    if !options.planner_context.is_empty() {
        harness = harness.with_planner_contexts(options.planner_context);
    }

    harness.with_strategy(super::def::Strategy::PlanExecute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tool(name: &str) -> ToolDef {
        ToolDef::function::<Value, _, _>(
            name,
            "a test tool",
            serde_json::json!({"type": "object"}),
            |_args: Value| async move { Ok(Value::Null) },
        )
    }

    #[test]
    fn test_ref_to_value() {
        assert_eq!(
            Ref::new("fetch").to_value(),
            serde_json::json!({"$ref": "fetch"})
        );
    }

    #[test]
    fn test_ref_embeds_in_nested_args_via_into() {
        let args = serde_json::json!({"document": Value::from(Ref::new("fetch"))});
        assert_eq!(args, serde_json::json!({"document": {"$ref": "fetch"}}));
    }

    #[test]
    fn test_context_text_only_emits_text() {
        let value = Context::text("be concise").to_value();
        assert_eq!(value, serde_json::json!({"text": "be concise"}));
    }

    #[test]
    fn test_context_url_emits_defaults_omitted() {
        let value = Context::url("https://example.com/rules").to_value();
        assert_eq!(
            value,
            serde_json::json!({"url": "https://example.com/rules"})
        );
    }

    #[test]
    fn test_context_url_with_all_options() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_owned(), "${GH_TOKEN}".to_owned());
        let value = Context::url("https://example.com/rules")
            .with_headers(headers)
            .with_required(false)
            .with_max_bytes(1024)
            .to_value();
        assert_eq!(
            value,
            serde_json::json!({
                "url": "https://example.com/rules",
                "headers": {"Authorization": "${GH_TOKEN}"},
                "required": false,
                "maxBytes": 1024,
            })
        );
    }

    #[test]
    fn test_generate_to_value_minimal() {
        let value = Generate::new("write the intro", r#"{"content": "..."}"#).to_value();
        assert_eq!(
            value,
            serde_json::json!({
                "instructions": "write the intro",
                "output_schema": r#"{"content": "..."}"#,
            })
        );
    }

    #[test]
    fn test_generate_with_ref_context() {
        let value = Generate::new("summarize", "{}")
            .with_context(Ref::new("fetch"))
            .to_value();
        assert_eq!(value["context"], serde_json::json!({"$ref": "fetch"}));
    }

    #[test]
    fn test_op_with_args_to_value() {
        let op = Op::with_args("create_directory", serde_json::json!({"path": "out"}));
        assert_eq!(
            op.to_value(),
            serde_json::json!({"tool": "create_directory", "args": {"path": "out"}})
        );
    }

    #[test]
    fn test_op_with_generate_to_value() {
        let op = Op::with_generate(
            "write_file",
            Generate::new("write the intro", r#"{"path": "out/intro.md"}"#),
        );
        let value = op.to_value();
        assert_eq!(value["tool"], "write_file");
        assert!(value.get("generate").is_some());
        assert!(value.get("args").is_none());
    }

    #[test]
    fn test_step_to_value_minimal() {
        let step = Step::new(
            "setup",
            vec![Op::with_args(
                "create_directory",
                serde_json::json!({"path": "out"}),
            )],
        );
        let value = step.to_value();
        assert_eq!(value["id"], "setup");
        assert!(value.get("depends_on").is_none());
        assert!(value.get("parallel").is_none());
    }

    #[test]
    fn test_step_with_depends_on_and_parallel() {
        let step = Step::new("write_sections", vec![])
            .with_depends_on(["setup"])
            .with_parallel(true);
        let value = step.to_value();
        assert_eq!(value["depends_on"], serde_json::json!(["setup"]));
        assert_eq!(value["parallel"], Value::Bool(true));
    }

    #[test]
    fn test_validation_to_value() {
        let validation = Validation::new("check_word_count")
            .with_args(serde_json::json!({"path": "out/intro.md", "min_words": 200}))
            .with_success_condition("$.passed === true");
        let value = validation.to_value();
        assert_eq!(value["tool"], "check_word_count");
        assert_eq!(value["success_condition"], "$.passed === true");
    }

    #[test]
    fn test_action_to_value() {
        let action = Action::new("notify").with_args(serde_json::json!({"channel": "#ops"}));
        let value = action.to_value();
        assert_eq!(value["tool"], "notify");
        assert_eq!(value["args"], serde_json::json!({"channel": "#ops"}));
    }

    #[test]
    fn test_plan_to_value_matches_python_shape() {
        let plan = Plan::new(vec![
            Step::new(
                "setup",
                vec![Op::with_args(
                    "create_directory",
                    serde_json::json!({"path": "out"}),
                )],
            ),
            Step::new(
                "write_sections",
                vec![Op::with_generate(
                    "write_file",
                    Generate::new(
                        "Write the introduction.",
                        r#"{"path": "out/intro.md", "content": "..."}"#,
                    ),
                )],
            )
            .with_depends_on(["setup"])
            .with_parallel(true),
        ])
        .with_validation(vec![Validation::new("check_word_count")
            .with_args(serde_json::json!({"path": "out/intro.md", "min_words": 200}))]);

        let value = plan.to_value();
        assert_eq!(value["steps"].as_array().unwrap().len(), 2);
        assert_eq!(value["validation"].as_array().unwrap().len(), 1);
        assert!(value.get("on_success").is_none());
        assert!(value.get("on_failure").is_none());
    }

    #[test]
    fn test_plan_omits_empty_collections() {
        let plan = Plan::new(vec![Step::new("only", vec![])]);
        let value = plan.to_value();
        let obj = value.as_object().unwrap();
        assert!(!obj.contains_key("validation"));
        assert!(!obj.contains_key("on_success"));
        assert!(!obj.contains_key("on_failure"));
    }

    #[test]
    fn test_plan_execute_builds_planner_and_parent() {
        let agent = plan_execute(
            "research",
            vec![test_tool("search")],
            PlanExecuteOptions {
                planner_instructions: "Plan a research report.".to_owned(),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(agent.name, "research");
        assert_eq!(agent.strategy, super::super::def::Strategy::PlanExecute);
        assert_eq!(agent.planner.as_ref().unwrap().name, "research_planner");
        assert!(agent.fallback.is_none());
        assert_eq!(agent.tools.len(), 1);
    }

    #[test]
    fn test_plan_execute_builds_fallback_when_instructions_given() {
        let agent = plan_execute(
            "research",
            vec![test_tool("search")],
            PlanExecuteOptions {
                fallback_instructions: Some("Recover and retry.".to_owned()),
                fallback_max_turns: Some(5),
                ..Default::default()
            },
        )
        .unwrap();

        let fallback = agent.fallback.as_ref().unwrap();
        assert_eq!(fallback.name, "research_fallback");
        assert_eq!(fallback.tools.len(), 1);
        assert_eq!(agent.fallback_max_turns, Some(5));
    }

    #[test]
    fn test_plan_execute_propagates_model_to_planner_fallback_and_parent() {
        let agent = plan_execute(
            "research",
            vec![test_tool("search")],
            PlanExecuteOptions {
                fallback_instructions: Some("Recover.".to_owned()),
                model: Some("openai/gpt-4o".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(agent.model, Some("openai/gpt-4o".to_owned()));
        assert_eq!(
            agent.planner.as_ref().unwrap().model,
            Some("openai/gpt-4o".to_owned())
        );
        assert_eq!(
            agent.fallback.as_ref().unwrap().model,
            Some("openai/gpt-4o".to_owned())
        );
    }

    #[test]
    fn test_plan_execute_fails_without_tools() {
        // Matches Strategy::PlanExecute's own tools-required check (AgentDef::with_strategy) --
        // plan_execute() doesn't bypass it.
        let result = plan_execute("research", vec![], PlanExecuteOptions::default());
        result.unwrap_err();
    }
}
