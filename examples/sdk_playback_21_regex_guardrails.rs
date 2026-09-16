//! Reproduces `llm-recordings/21_regex_guardrails` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/21_regex_guardrails.py`.
//!
//! `cargo run --features agents --example sdk_playback_21_regex_guardrails`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Guardrail, OnFail, Position, RegexGuardrail, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use support::run_with_local_tools;

#[derive(Deserialize)]
struct UserArgs {
    #[allow(dead_code)]
    user_id: String,
}

fn build_guardrails() -> Result<Vec<Guardrail>> {
    let no_emails = Guardrail::new(
        "no_email_addresses",
        RegexGuardrail::new([r"[\w.+-]+@[\w-]+\.[\w.-]+"])?
            .with_message("Response must not contain email addresses. Redact them."),
    )
    .with_position(Position::Output)?
    .with_on_fail(OnFail::Retry)?;

    let no_ssn = Guardrail::new(
        "no_ssn",
        RegexGuardrail::new([r"\b\d{3}-\d{2}-\d{4}\b"])?
            .with_message("Response must not contain Social Security Numbers."),
    )
    .with_position(Position::Output)?
    .with_on_fail(OnFail::Raise)?;

    Ok(vec![no_emails, no_ssn])
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let get_user_profile = ToolDef::function(
        "get_user_profile",
        "Retrieve a user's profile from the database.",
        json!({
            "type": "object",
            "properties": { "user_id": { "type": "string" } },
            "required": ["user_id"]
        }),
        |_args: UserArgs| async move {
            Ok(json!({
                "name": "Alice Johnson",
                "email": "alice.johnson@example.com",
                "ssn": "123-45-6789",
                "department": "Engineering",
                "role": "Senior Developer"
            }))
        },
    );

    let mut agent = AgentDef::new("hr_assistant")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are an HR assistant. When asked about employees, look up their \
             profile and share ALL the details you find.",
        )
        .with_tool(get_user_profile);
    for g in build_guardrails()? {
        agent = agent.with_guardrail(g);
    }

    println!("=== Scenario 1: Request PII — guardrails trigger ===");
    let result = run_with_local_tools(
        &config,
        &agent,
        Value::String("Tell me everything about user U-001.".into()),
    )
    .await?;
    println!("status: {}", result.status);
    println!("output: {}", result.output);

    println!("\n=== Scenario 2: Non-PII question — guardrails pass ===");
    let mut clean_agent = AgentDef::new("dept_assistant")?
        .with_model("mock/mockLLM")
        .with_instructions("You are an HR assistant. Answer questions about departments.");
    for g in build_guardrails()? {
        clean_agent = clean_agent.with_guardrail(g);
    }

    let result2 = run_with_local_tools(
        &config,
        &clean_agent,
        Value::String("What departments exist at the company?".into()),
    )
    .await?;
    println!("status: {}", result2.status);
    println!("output: {}", result2.output);

    Ok(())
}
