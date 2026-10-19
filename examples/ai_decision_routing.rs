// Copyright 2026 Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Route a request with one synchronous decision-backed SWITCH and an inline branch.
//! No Rust worker is needed. Provider failures fail the workflow without task retries.
//! `Run: CONDUCTOR_SERVER_URL=http://localhost:8081/api CONDUCTOR_DECISION_PROVIDER=openrouter`
//! `cargo run --example ai_decision_routing -- "I was charged twice on my latest invoice."`

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::{ConductorError, Result},
    models::{StartWorkflowRequest, WorkflowDef, WorkflowTask},
};
use serde_json::json;
use std::time::{Duration, Instant};

fn workflow() -> WorkflowDef {
    let mut decision = WorkflowTask::switch("route_request", "route")
        .with_input_param(
            "model",
            std::env::var("CONDUCTOR_DECISION_MODEL").unwrap_or_else(|_| "jev-1.13".to_owned()),
        )
        .with_input_param("state", "${workflow.input.request}")
        .with_input_param(
            "questions",
            json!({"route": {
                "type": "choice",
                "instructions": "Choose the team best suited to handle this request.",
                "choices": {
                    "billing": "Payments, invoices, refunds, or subscriptions.",
                    "technical": "Errors, outages, or product troubleshooting."
                }
            }}),
        );
    decision.evaluator_type = Some("decision".to_owned());
    if let Ok(provider) = std::env::var("CONDUCTOR_DECISION_PROVIDER") {
        decision = decision.with_input_param("provider", provider);
    }
    let billing = WorkflowTask::inline(
        "handle_billing",
        "({team: 'billing', nextAction: 'review_invoice', message: 'Check invoice line items and payment records.', request: $.request})",
    )
    .with_input_param("request", "${workflow.input.request}");
    let technical = WorkflowTask::inline(
        "handle_technical",
        "({team: 'technical', nextAction: 'collect_diagnostics', message: 'Collect error logs and steps to reproduce.', request: $.request})",
    )
    .with_input_param("request", "${workflow.input.request}");
    WorkflowDef::new("ai_decision_routing_rust")
        .with_version(1)
        .with_task(
            decision
                .with_switch_case("billing", vec![billing])
                .with_switch_case("technical", vec![technical]),
        )
        .with_task(
            WorkflowTask::inline("selected_result", "$.billing || $.technical")
                .with_input_param("billing", "${handle_billing.output.result}")
                .with_input_param("technical", "${handle_technical.output.result}"),
        )
        .with_output_param("selectedCase", "${route_request.output.selectedCase}")
        .with_output_param("answers", "${route_request.output.answers}")
        .with_output_param("usage", "${route_request.output.usage}")
        .with_output_param("cost", "${route_request.output.usage.cost}")
        .with_output_param("latencyMs", "${route_request.output.latencyMs}")
        .with_output_param("result", "${selected_result.output.result}")
}

#[tokio::main]
async fn main() -> Result<()> {
    let request = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "I was charged twice on my latest invoice.".to_owned());
    let client = ConductorClient::new(Configuration::from_env())?;
    let definition = workflow();
    client
        .metadata_client()
        .register_or_update_workflow_def(&definition, true)
        .await?;
    let workflows = client.workflow_client();
    let id = workflows
        .start_workflow(
            &StartWorkflowRequest::new(&definition.name)
                .with_version(1)
                .with_input_value("request", request),
        )
        .await?;
    println!("Workflow: {id}");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let result = workflows.get_workflow(&id, false).await?;
        if result.is_terminal() {
            if !result.is_successful() {
                return Err(ConductorError::task_execution(format!(
                    "{:?}: {:?}",
                    result.status, result.reason_for_incompletion
                )));
            }
            println!("{}", serde_json::to_string_pretty(&result.output)?);
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ConductorError::task_execution(format!(
                "Timed out waiting for {id}"
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_decision_switch_preserves_expression_and_cases() {
        let definition = serde_json::to_value(super::workflow()).unwrap();
        let decision = &definition["tasks"][0];
        assert_eq!(decision["type"], "SWITCH");
        assert_eq!(decision["evaluatorType"], "decision");
        assert_eq!(decision["expression"], "route");
        assert_eq!(
            decision["inputParameters"]["state"],
            "${workflow.input.request}"
        );
        assert!(decision["inputParameters"].get("switchCaseValue").is_none());
        let javascript = serde_json::to_value(conductor::models::WorkflowTask::switch(
            "legacy", "$.choice",
        ))
        .unwrap();
        assert_eq!(javascript["evaluatorType"], "javascript");
        assert_eq!(javascript["expression"], "$.choice");
        for team in ["billing", "technical"] {
            assert!(decision["decisionCases"][team].is_array());
        }
    }
}
