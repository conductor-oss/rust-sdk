//! Reproduces `llm-recordings/103_plan_and_compile` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/103_plan_and_compile.py`, minus
//! its post-run `PLAN_AND_COMPILE` task inspection (that just reads back what the server
//! compiled; nothing to reproduce for playback purposes). Both recorded LLM calls are entirely
//! server-driven (the planner call, and the `generate` op's structured-output call for
//! `write_summary`'s `text` arg) — nothing about them needs client-side handling beyond
//! `factorial`/`write_summary`/`check_summary` being real, locally-served tools.
//!
//! `cargo run --features agents --example sdk_playback_103_plan_and_compile`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{plan_execute, PlanExecuteOptions, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use support::run_with_local_tools;

#[derive(Deserialize)]
struct FactorialArgs {
    n: i64,
}

#[derive(Deserialize)]
struct SummaryArgs {
    text: String,
}

#[derive(Deserialize)]
struct CheckSummaryArgs {
    text: String,
    min_chars: i64,
}

// Rust's `\`-newline string continuation strips leading whitespace on the *next* line (unlike
// python's triple-quoted strings, which preserve it verbatim) -- the 3-space indent under items
// 2 and 3 below is written as an explicit `\n   ` rather than relying on source indentation, to
// exactly reproduce python's `PLANNER_INSTRUCTIONS` string (a first attempt that leaned on
// source indentation silently lost that whitespace and failed the mock provider's exact-request
// match on the planner's first turn).
const PLANNER_INSTRUCTIONS: &str = "You are a math-explainer planner. Plan a workflow that:\n\n1. Computes factorials of 1, 2, 3, 4, 5 in PARALLEL using ``factorial`` (static args).\n2. Writes a short prose summary about factorial growth using ``write_summary``\n   (use a ``generate`` block — the LLM produces the ``text`` arg at run time).\n3. Validates the summary is at least 30 characters via ``check_summary``,\n   with ``success_condition: \"$.passed === true\"``.\n";

fn factorial(n: i64) -> String {
    if !(0..=20).contains(&n) {
        return format!("ERROR: n must be in [0, 20], got {n}");
    }
    (1..=n).product::<i64>().max(1).to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    // Descriptions below reproduce python's `@tool`-decorated functions' *full* docstrings
    // verbatim (not just the first line) -- python's `@tool` uses the whole docstring,
    // "Args:" section included, as the tool description, and that full text is what the
    // server's planner-prompt "## Available tools" block embeds.
    let factorial_tool = ToolDef::function(
        "factorial",
        "Compute n! and return it as a string.\n\nArgs:\n    n: Non-negative integer. Capped at 20 to keep things sane.",
        json!({
            "type": "object",
            "properties": { "n": { "type": "integer" } },
            "required": ["n"]
        }),
        |args: FactorialArgs| async move { Ok(Value::String(factorial(args.n))) },
    );

    let write_summary = ToolDef::function(
        "write_summary",
        "Persist a short summary string. Returns it back for the validator.",
        json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": ["text"]
        }),
        |args: SummaryArgs| async move { Ok(Value::String(args.text)) },
    );

    let check_summary = ToolDef::function(
        "check_summary",
        "Return JSON ``{passed, length, min_chars}`` for the validator.\n\nArgs:\n    text: The summary to check.\n    min_chars: Minimum acceptable length in characters.",
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" },
                "min_chars": { "type": "integer" }
            },
            "required": ["text", "min_chars"]
        }),
        |args: CheckSummaryArgs| async move {
            let passed = args.text.len() as i64 >= args.min_chars;
            Ok(Value::String(
                json!({ "passed": passed, "length": args.text.len(), "min_chars": args.min_chars })
                    .to_string(),
            ))
        },
    );

    let harness = plan_execute(
        "plan_and_compile_demo",
        vec![factorial_tool, write_summary, check_summary],
        PlanExecuteOptions {
            planner_instructions: PLANNER_INSTRUCTIONS.to_string(),
            fallback_instructions: Some(
                "The plan failed. Use the available tools to recover.".to_string(),
            ),
            model: Some("mock/mockLLM".to_string()),
            fallback_max_turns: Some(4),
            planner_context: Vec::new(),
        },
    )?;

    let result =
        run_with_local_tools(&config, &harness, Value::String("Topic: factorials".into())).await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
