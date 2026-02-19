// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{
        ChatMessage, StartWorkflowRequest, WorkflowDef, WorkflowStatus, WorkflowTask,
        WorkflowTimeoutPolicy,
    },
};

// Configuration
const LLM_PROVIDER: &str = "openai";
const LLM_MODEL: &str = "gpt-4o-mini";

// Agent personas
const EXPERT_PERSONA: &str = r#"You are a technical expert who provides detailed, accurate information.
Your role is to explain concepts thoroughly and back up claims with reasoning.
Keep responses focused and under 150 words.
Respond directly to the topic or previous message."#;

const CRITIC_PERSONA: &str = r#"You are a critical thinker who challenges assumptions and asks probing questions.
Your role is to identify potential issues, edge cases, and areas needing clarification.
Be constructive but thorough in your critique.
Keep responses focused and under 150 words.
Respond to the previous expert's statement."#;

const SYNTHESIZER_PERSONA: &str = r#"You are a synthesizer who combines different perspectives into clear conclusions.
Your role is to summarize the discussion and provide actionable insights.
Highlight key agreements, disagreements, and recommendations.
Keep responses focused and under 200 words."#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Multi-Agent Chat Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    // ==========================================================================
    // Create Multi-Agent Discussion Workflow
    // ==========================================================================
    println!("\nCREATING MULTI-AGENT WORKFLOW");
    println!("{}", "=".repeat(80));
    println!();

    let workflow_name = "rust_multiagent_discussion";

    // Round 1: Expert provides initial analysis
    let expert_round1 =
        WorkflowTask::llm_chat_complete("expert_round1_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(EXPERT_PERSONA),
                ChatMessage::user("Topic for discussion: ${workflow.input.topic}\n\nProvide your expert analysis on this topic."),
            ])
            .with_temperature(0.7)
            .with_max_tokens(300);

    // Round 1: Critic responds to expert
    let critic_round1 =
        WorkflowTask::llm_chat_complete("critic_round1_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(CRITIC_PERSONA),
                ChatMessage::user("Topic: ${workflow.input.topic}\n\nExpert's analysis:\n${expert_round1_ref.output.result}\n\nProvide your critical response."),
            ])
            .with_temperature(0.7)
            .with_max_tokens(300);

    // Round 2: Expert addresses critique
    let expert_round2 =
        WorkflowTask::llm_chat_complete("expert_round2_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(EXPERT_PERSONA),
                ChatMessage::user("Topic: ${workflow.input.topic}\n\nYour initial analysis:\n${expert_round1_ref.output.result}\n\nCritic's response:\n${critic_round1_ref.output.result}\n\nAddress the critique and provide additional insights."),
            ])
            .with_temperature(0.7)
            .with_max_tokens(300);

    // Round 2: Critic's final thoughts
    let critic_round2 =
        WorkflowTask::llm_chat_complete("critic_round2_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(CRITIC_PERSONA),
                ChatMessage::user("Topic: ${workflow.input.topic}\n\nExpert's follow-up:\n${expert_round2_ref.output.result}\n\nProvide your final critical assessment."),
            ])
            .with_temperature(0.7)
            .with_max_tokens(300);

    // Synthesizer: Combine all perspectives
    let synthesizer = WorkflowTask::llm_chat_complete("synthesizer_ref", LLM_PROVIDER, LLM_MODEL)
        .with_messages(vec![
            ChatMessage::system(SYNTHESIZER_PERSONA),
            ChatMessage::user(
                r#"Topic: ${workflow.input.topic}

Full Discussion:

EXPERT (Round 1):
${expert_round1_ref.output.result}

CRITIC (Round 1):
${critic_round1_ref.output.result}

EXPERT (Round 2):
${expert_round2_ref.output.result}

CRITIC (Round 2):
${critic_round2_ref.output.result}

Synthesize this discussion into key takeaways and recommendations."#,
            ),
        ])
        .with_temperature(0.5)
        .with_max_tokens(400);

    // Format the final output
    let format_script = r#"
    (function(){
        return {
            topic: $.topic,
            discussion: {
                expert_round1: $.expert1,
                critic_round1: $.critic1,
                expert_round2: $.expert2,
                critic_round2: $.critic2
            },
            synthesis: $.synthesis,
            total_rounds: 2
        };
    })();
    "#;

    let format_task = WorkflowTask::inline("format_output_ref", format_script)
        .with_input_param("topic", "${workflow.input.topic}")
        .with_input_param("expert1", "${expert_round1_ref.output.result}")
        .with_input_param("critic1", "${critic_round1_ref.output.result}")
        .with_input_param("expert2", "${expert_round2_ref.output.result}")
        .with_input_param("critic2", "${critic_round2_ref.output.result}")
        .with_input_param("synthesis", "${synthesizer_ref.output.result}");

    // Build the workflow
    let workflow = WorkflowDef::new(workflow_name)
        .with_description("Multi-agent discussion with expert, critic, and synthesizer")
        .with_version(1)
        .with_task(expert_round1)
        .with_task(critic_round1)
        .with_task(expert_round2)
        .with_task(critic_round2)
        .with_task(synthesizer)
        .with_task(format_task)
        .with_input_parameters(vec!["topic".to_string()])
        .with_output_param("topic", "${format_output_ref.output.result.topic}")
        .with_output_param(
            "discussion",
            "${format_output_ref.output.result.discussion}",
        )
        .with_output_param("synthesis", "${format_output_ref.output.result.synthesis}")
        .with_timeout(180, WorkflowTimeoutPolicy::TimeOutWf);

    println!("Workflow: {}", workflow.name);
    println!("Description: Multi-agent discussion system");
    println!();
    println!("Agents:");
    println!("  1. Expert     - Provides detailed technical analysis");
    println!("  2. Critic     - Challenges and questions assumptions");
    println!("  3. Synthesizer - Combines insights into conclusions");
    println!();
    println!("Flow:");
    println!("  Expert (R1) → Critic (R1) → Expert (R2) → Critic (R2) → Synthesizer");
    println!();

    // Register the workflow
    println!("Registering workflow...");
    metadata_client
        .register_or_update_workflow_def(&workflow, true)
        .await?;
    println!("  Workflow registered: {}", workflow_name);

    // ==========================================================================
    // Run Example Discussion
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("RUNNING EXAMPLE DISCUSSION");
    println!("{}", "=".repeat(80));
    println!();

    let discussion_topic = "Should companies adopt Rust for their backend services?";
    println!("Topic: {}\n", discussion_topic);

    let request = StartWorkflowRequest::new(workflow_name)
        .with_version(1)
        .with_input_value("topic", discussion_topic);

    match workflow_client.start_workflow(&request).await {
        Ok(workflow_id) => {
            println!("Discussion started: {}", workflow_id);
            println!();

            // Poll for completion
            println!("Waiting for agents to complete discussion...");
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(120);

            loop {
                if start.elapsed() > timeout {
                    println!("Timeout waiting for discussion to complete");
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                match workflow_client.get_workflow(&workflow_id, true).await {
                    Ok(wf) => {
                        let status = wf.status;
                        print!("\r  Status: {:?}          ", status);

                        if status == WorkflowStatus::Completed {
                            println!();
                            println!();
                            println!("{}", "=".repeat(80));
                            println!("DISCUSSION RESULTS");
                            println!("{}", "=".repeat(80));

                            if let Some(discussion) = wf.output.get("discussion") {
                                // Expert Round 1
                                if let Some(expert1) = discussion.get("expert_round1") {
                                    println!("\n--- EXPERT (Round 1) ---");
                                    println!("{}", expert1.as_str().unwrap_or("N/A"));
                                }

                                // Critic Round 1
                                if let Some(critic1) = discussion.get("critic_round1") {
                                    println!("\n--- CRITIC (Round 1) ---");
                                    println!("{}", critic1.as_str().unwrap_or("N/A"));
                                }

                                // Expert Round 2
                                if let Some(expert2) = discussion.get("expert_round2") {
                                    println!("\n--- EXPERT (Round 2) ---");
                                    println!("{}", expert2.as_str().unwrap_or("N/A"));
                                }

                                // Critic Round 2
                                if let Some(critic2) = discussion.get("critic_round2") {
                                    println!("\n--- CRITIC (Round 2) ---");
                                    println!("{}", critic2.as_str().unwrap_or("N/A"));
                                }
                            }

                            // Synthesis
                            if let Some(synthesis) = wf.output.get("synthesis") {
                                println!("\n{}", "=".repeat(80));
                                println!("SYNTHESIS");
                                println!("{}", "=".repeat(80));
                                println!("{}", synthesis.as_str().unwrap_or("N/A"));
                            }

                            break;
                        } else if matches!(
                            status,
                            WorkflowStatus::Failed
                                | WorkflowStatus::Terminated
                                | WorkflowStatus::TimedOut
                        ) {
                            println!();
                            println!("  Discussion failed: {:?}", status);
                            if let Some(reason) = wf.reason_for_incompletion {
                                println!("  Reason: {}", reason);
                            }
                            break;
                        }
                    }
                    Err(e) => {
                        println!("  Error checking status: {}", e);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            println!("Could not start discussion: {}", e);
            println!();
            println!("This is expected if LLM integration is not configured.");
        }
    }

    // ==========================================================================
    // Alternative Configurations
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("ALTERNATIVE CONFIGURATIONS");
    println!("{}", "=".repeat(80));
    println!();

    println!("You can customize the multi-agent system by:");
    println!();
    println!("1. Changing Agent Personas:");
    println!("   - Optimist vs Pessimist");
    println!("   - Developer vs Security Expert");
    println!("   - Designer vs Engineer");
    println!();
    println!("2. Adding More Rounds:");
    println!("   - Use Do-While loops for dynamic round counts");
    println!("   - Store conversation history in workflow variables");
    println!();
    println!("3. Parallel Agent Execution:");
    println!("   - Use Fork/Join for simultaneous responses");
    println!("   - Collect and aggregate multiple perspectives");
    println!();
    println!("4. Human-in-the-Loop:");
    println!("   - Add WAIT tasks for human input between rounds");
    println!("   - Allow moderator to guide the discussion");

    // ==========================================================================
    // Cleanup
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CLEANUP");
    println!("{}", "=".repeat(80));
    println!();

    match metadata_client.delete_workflow_def(workflow_name, 1).await {
        Ok(_) => println!("  Deleted workflow: {}", workflow_name),
        Err(e) => println!("  Could not delete workflow: {}", e),
    }

    println!();
    println!("Multi-agent chat example completed!");

    Ok(())
}
