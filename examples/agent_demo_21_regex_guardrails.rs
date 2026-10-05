// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, Guardrail, OnFail, Position, RegexGuardrail, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct UserArgs {
    #[expect(dead_code)]
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
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
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
        .with_model(model.clone())
        .with_instructions(
            "You are an HR assistant. When asked about employees, look up their \
             profile and share ALL the details you find.",
        )
        .with_tool(get_user_profile);
    for g in build_guardrails()? {
        agent = agent.with_guardrail(g);
    }

    println!("=== Scenario 1: Request PII \u{2014} guardrails trigger ===");
    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&agent).await?;
    let result = runtime
        .run(
            &agent,
            Value::String("Tell me everything about user U-001.".into()),
        )
        .await;
    runtime.shutdown().await?;
    let result = result?;
    println!("status: {}", result.status);
    println!("output: {}", result.output);

    println!("\n=== Scenario 2: Non-PII question \u{2014} guardrails pass ===");
    let mut clean_agent = AgentDef::new("dept_assistant")?
        .with_model(model)
        .with_instructions("You are an HR assistant. Answer questions about departments.");
    for g in build_guardrails()? {
        clean_agent = clean_agent.with_guardrail(g);
    }

    let runtime2 = AgentRuntime::new(config.clone())?;
    let result2 = runtime2
        .run(
            &clean_agent,
            Value::String("What departments exist at the company?".into()),
        )
        .await;
    let result2 = result2?;
    println!("status: {}", result2.status);
    println!("output: {}", result2.output);

    Ok(())
}
