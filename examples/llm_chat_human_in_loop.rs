//! LLM Chat with Human-in-the-Loop Example
//!
//! This example demonstrates an interactive chat workflow where the LLM
//! generates responses and then waits for human input to continue.
//!
//! ## Architecture
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │                 HUMAN-IN-THE-LOOP CHAT WORKFLOW                      │
//! ├──────────────────────────────────────────────────────────────────────┤
//! │                                                                      │
//! │  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐         │
//! │  │  LLM     │──▶│  WAIT    │──▶│  LLM     │──▶│  WAIT    │──▶ ...  │
//! │  │ Response │   │(Human)   │   │ Response │   │(Human)   │         │
//! │  └──────────┘   └──────────┘   └──────────┘   └──────────┘         │
//! │                      │                              │               │
//! │                      ▼                              ▼               │
//! │              Human provides              Human provides             │
//! │              next message                next message               │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Use Cases
//! - Interactive chatbots requiring approval
//! - Customer support with human escalation
//! - Content generation with human review
//! - Step-by-step guided processes
//!
//! ## Prerequisites
//! 1. Conductor server with AI/LLM support
//! 2. LLM provider configured
//!
//! ## Environment Variables
//! ```bash
//! export CONDUCTOR_SERVER_URL="https://your-conductor-server/api"
//! export CONDUCTOR_AUTH_KEY="your-key"
//! export CONDUCTOR_AUTH_SECRET="your-secret"
//! ```
//!
//! ## Run
//! ```bash
//! cargo run --example llm_chat_human_in_loop
//! ```

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{
        ChatMessage, StartWorkflowRequest, TaskResultStatus, TaskStatus, WorkflowDef,
        WorkflowStatus, WorkflowTask, WorkflowTimeoutPolicy,
    },
};

// Configuration
const LLM_PROVIDER: &str = "openai";
const LLM_MODEL: &str = "gpt-4o-mini";

const ASSISTANT_PERSONA: &str = r#"You are a helpful assistant engaged in a conversation.
Keep your responses concise and engaging.
Ask follow-up questions to keep the conversation going.
If the user wants to end the conversation, say goodbye politely."#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("LLM Chat Human-in-the-Loop Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();
    let task_client = client.task_client();

    // ==========================================================================
    // Create Human-in-the-Loop Chat Workflow
    // ==========================================================================
    println!("\nCREATING HUMAN-IN-THE-LOOP WORKFLOW");
    println!("{}", "=".repeat(80));
    println!();

    let workflow_name = "rust_interactive_chat";

    // Initial LLM response to user's first message
    let initial_response =
        WorkflowTask::llm_chat_complete("initial_response_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(ASSISTANT_PERSONA),
                ChatMessage::user("${workflow.input.initial_message}"),
            ])
            .with_temperature(0.7)
            .with_max_tokens(200);

    // Wait for human input (turn 1)
    let wait_turn1 = WorkflowTask::wait("wait_for_human_1_ref")
        .with_description("Waiting for human's second message");

    // LLM response to turn 1
    let response_turn1 =
        WorkflowTask::llm_chat_complete("response_turn1_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(ASSISTANT_PERSONA),
                ChatMessage::user("${workflow.input.initial_message}"),
                ChatMessage::assistant("${initial_response_ref.output.result}"),
                ChatMessage::user("${wait_for_human_1_ref.output.human_message}"),
            ])
            .with_temperature(0.7)
            .with_max_tokens(200);

    // Wait for human input (turn 2)
    let wait_turn2 = WorkflowTask::wait("wait_for_human_2_ref")
        .with_description("Waiting for human's third message");

    // LLM response to turn 2
    let response_turn2 =
        WorkflowTask::llm_chat_complete("response_turn2_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(ASSISTANT_PERSONA),
                ChatMessage::user("${workflow.input.initial_message}"),
                ChatMessage::assistant("${initial_response_ref.output.result}"),
                ChatMessage::user("${wait_for_human_1_ref.output.human_message}"),
                ChatMessage::assistant("${response_turn1_ref.output.result}"),
                ChatMessage::user("${wait_for_human_2_ref.output.human_message}"),
            ])
            .with_temperature(0.7)
            .with_max_tokens(200);

    // Format the conversation history
    let format_script = r#"
    (function(){
        var conversation = [];
        
        // Turn 0 (initial)
        conversation.push({
            turn: 0,
            user: $.initial_message,
            assistant: $.initial_response
        });
        
        // Turn 1
        if ($.human_msg_1) {
            conversation.push({
                turn: 1,
                user: $.human_msg_1,
                assistant: $.response_1
            });
        }
        
        // Turn 2
        if ($.human_msg_2) {
            conversation.push({
                turn: 2,
                user: $.human_msg_2,
                assistant: $.response_2
            });
        }
        
        return {
            conversation: conversation,
            total_turns: conversation.length,
            final_assistant_message: $.response_2 || $.response_1 || $.initial_response
        };
    })();
    "#;

    let format_task = WorkflowTask::inline("format_conversation_ref", format_script)
        .with_input_param("initial_message", "${workflow.input.initial_message}")
        .with_input_param("initial_response", "${initial_response_ref.output.result}")
        .with_input_param("human_msg_1", "${wait_for_human_1_ref.output.human_message}")
        .with_input_param("response_1", "${response_turn1_ref.output.result}")
        .with_input_param("human_msg_2", "${wait_for_human_2_ref.output.human_message}")
        .with_input_param("response_2", "${response_turn2_ref.output.result}");

    // Build the workflow
    let workflow = WorkflowDef::new(workflow_name)
        .with_description("Interactive chat with human-in-the-loop using WAIT tasks")
        .with_version(1)
        .with_task(initial_response)
        .with_task(wait_turn1)
        .with_task(response_turn1)
        .with_task(wait_turn2)
        .with_task(response_turn2)
        .with_task(format_task)
        .with_input_parameters(vec!["initial_message".to_string()])
        .with_output_param("conversation", "${format_conversation_ref.output.result.conversation}")
        .with_output_param("total_turns", "${format_conversation_ref.output.result.total_turns}")
        .with_timeout(3600, WorkflowTimeoutPolicy::TimeOutWf); // 1 hour timeout for human input

    println!("Workflow: {}", workflow.name);
    println!("Description: Interactive chat with WAIT tasks for human input");
    println!();
    println!("Flow:");
    println!("  1. User sends initial message");
    println!("  2. LLM responds");
    println!("  3. WAIT for human's next message (external signal)");
    println!("  4. LLM responds to conversation");
    println!("  5. WAIT for human's next message");
    println!("  6. LLM provides final response");
    println!();

    // Register the workflow
    println!("Registering workflow...");
    metadata_client
        .register_or_update_workflow_def(&workflow, true)
        .await?;
    println!("  Workflow registered: {}", workflow_name);

    // ==========================================================================
    // Run Interactive Chat Demo
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("INTERACTIVE CHAT DEMO");
    println!("{}", "=".repeat(80));
    println!();

    let initial_message = "Hello! I'm learning about workflow orchestration. Can you explain what Conductor is?";
    println!("User: {}\n", initial_message);

    let request = StartWorkflowRequest::new(workflow_name)
        .with_version(1)
        .with_input_value("initial_message", initial_message);

    match workflow_client.start_workflow(&request).await {
        Ok(workflow_id) => {
            println!("Workflow started: {}", workflow_id);
            println!();

            // Wait for initial response
            println!("Waiting for LLM's initial response...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

            // Check status and get initial response
            let wf = workflow_client.get_workflow(&workflow_id, true).await?;

            // Find the initial response task output
            if let Some(initial_task) = wf
                .tasks
                .iter()
                .find(|t| t.reference_task_name == "initial_response_ref")
            {
                if let Some(result) = initial_task.output_data.get("result") {
                    println!("Assistant: {}\n", result.as_str().unwrap_or("..."));
                }
            }

            // Check if waiting for human input
            if let Some(wait_task) = wf
                .tasks
                .iter()
                .find(|t| t.reference_task_name == "wait_for_human_1_ref")
            {
                if wait_task.status == TaskStatus::InProgress {
                    println!("{}", "-".repeat(60));
                    println!("WORKFLOW IS NOW WAITING FOR HUMAN INPUT");
                    println!("{}", "-".repeat(60));
                    println!();
                    println!("To continue the conversation, send a signal to the WAIT task:");
                    println!();
                    println!("Using API:");
                    println!("  POST /api/tasks/{}/update", wait_task.task_id);
                    println!("  Body: {{\"status\": \"COMPLETED\", \"outputData\": {{\"human_message\": \"Your message here\"}}}}");
                    println!();
                    println!("Using Rust SDK:");
                    println!("  task_client.update_task_sync(");
                    println!("      TaskResult::completed_with_output(json!({{\"human_message\": \"Your message\"}}))");
                    println!("          .with_task_id(\"{}\")", wait_task.task_id);
                    println!("          .with_workflow_id(\"{}\"),", workflow_id);
                    println!("  ).await?;");
                    println!();

                    // Simulate human input for demo
                    println!("{}", "=".repeat(80));
                    println!("SIMULATING HUMAN INPUT (for demo)");
                    println!("{}", "=".repeat(80));
                    println!();

                    let human_message_1 = "That's interesting! How does it handle task failures?";
                    println!("Simulated Human: {}\n", human_message_1);

                    // Update the wait task with human input using update_task_sync
                    let output = serde_json::json!({
                        "human_message": human_message_1
                    });

                    match task_client
                        .update_task_sync(
                            &workflow_id,
                            "wait_for_human_1_ref",
                            TaskResultStatus::Completed,
                            output,
                            None,
                        )
                        .await
                    {
                        Ok(_) => {
                            println!("Human input sent successfully!");
                            println!();

                            // Wait for LLM response
                            println!("Waiting for LLM's response...");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                            // Get updated workflow
                            let wf = workflow_client.get_workflow(&workflow_id, true).await?;

                            if let Some(response_task) = wf
                                .tasks
                                .iter()
                                .find(|t| t.reference_task_name == "response_turn1_ref")
                            {
                                if let Some(result) = response_task.output_data.get("result") {
                                    println!("Assistant: {}\n", result.as_str().unwrap_or("..."));
                                }
                            }

                            // Continue with second human input
                            if let Some(wait_task2) = wf
                                .tasks
                                .iter()
                                .find(|t| t.reference_task_name == "wait_for_human_2_ref")
                            {
                                if wait_task2.status == TaskStatus::InProgress {
                                    let human_message_2 =
                                        "Thanks! That's very helpful. Goodbye!";
                                    println!("Simulated Human: {}\n", human_message_2);

                                    let output2 = serde_json::json!({
                                        "human_message": human_message_2
                                    });

                                    task_client
                                        .update_task_sync(
                                            &workflow_id,
                                            "wait_for_human_2_ref",
                                            TaskResultStatus::Completed,
                                            output2,
                                            None,
                                        )
                                        .await?;

                                    // Wait for final response
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                                    // Get final workflow status
                                    let final_wf =
                                        workflow_client.get_workflow(&workflow_id, true).await?;

                                    if let Some(final_task) = final_wf
                                        .tasks
                                        .iter()
                                        .find(|t| t.reference_task_name == "response_turn2_ref")
                                    {
                                        if let Some(result) = final_task.output_data.get("result") {
                                            println!(
                                                "Assistant: {}\n",
                                                result.as_str().unwrap_or("...")
                                            );
                                        }
                                    }

                                    if final_wf.status == WorkflowStatus::Completed {
                                        println!("{}", "=".repeat(80));
                                        println!("CONVERSATION COMPLETED");
                                        println!("{}", "=".repeat(80));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("Could not update task: {}", e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Could not start workflow: {}", e);
            println!();
            println!("This is expected if LLM integration is not configured.");
        }
    }

    // ==========================================================================
    // Alternative: Using Webhooks
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("ALTERNATIVE: USING WEBHOOKS");
    println!("{}", "=".repeat(80));
    println!();

    println!("Instead of WAIT tasks, you can also use WAIT_FOR_WEBHOOK:");
    println!();
    println!("```rust");
    println!("// Create webhook wait task");
    println!("let webhook_wait = WorkflowTask::wait_for_webhook(\"wait_ref\", \"unique-match-key\")");
    println!("    .with_input_param(\"matches\", json!({{");
    println!("        \"$['session_id']\": \"${{workflow.input.session_id}}\"");
    println!("    }}));");
    println!();
    println!("// Client sends webhook to:");
    println!("// POST /api/webhook/unique-match-key");
    println!("// Body: {{\"session_id\": \"123\", \"message\": \"Hello\"}}");
    println!("```");

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
    println!("Human-in-the-loop chat example completed!");
    println!();
    println!("Key Concepts:");
    println!("  1. WAIT tasks pause workflow execution for external signals");
    println!("  2. Use task_client.update_task_sync() to resume WAIT tasks");
    println!("  3. Pass data through outputData when resuming");
    println!("  4. Conversation history is maintained across turns");
    println!("  5. Set appropriate timeouts for human response time");

    Ok(())
}
