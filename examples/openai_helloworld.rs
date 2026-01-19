//! OpenAI Hello World Example
//!
//! This example demonstrates how to use LLM tasks in Conductor workflows.
//! It mirrors the Python SDK's orkes/open_ai_helloworld.py example.
//!
//! Prerequisites:
//! - Conductor server running with OpenAI integration configured
//! - CONDUCTOR_SERVER_URL environment variable set
//! - CONDUCTOR_AUTH_KEY and CONDUCTOR_AUTH_SECRET for Orkes Cloud

use std::collections::HashMap;
use std::time::Duration;

use conductor::worker::{FnWorker, WorkerOutput};
use conductor::{
    Configuration, OrkesClients, StartWorkflowRequest, TaskHandler, WorkflowDef, WorkflowTask,
};

/// Worker to get the current user's name
async fn get_friend_name(_task: conductor::Task) -> conductor::Result<WorkerOutput> {
    let name = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "anonymous".to_string());

    Ok(WorkerOutput::completed_with_result(serde_json::json!({
        "result": name
    })))
}

fn get_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Configuration
    let llm_provider = format!("open_ai_{}", get_username());
    let text_complete_model = "gpt-4";
    // Note: embedding_model would be used for vector DB operations
    // let _embedding_model = "text-embedding-ada-002";

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config.clone())?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();
    let prompt_client = clients.get_prompt_client();
    let integration_client = clients.get_integration_client();

    // Start workers
    let mut handler = TaskHandler::new(config.clone())?;
    handler.add_worker(FnWorker::new("get_friends_name", get_friend_name));
    handler.start().await?;

    // Define and save prompt template
    let prompt_name = "say_hi_to_friend";
    let prompt_text = "give an evening greeting to ${friend_name}. go: ";

    prompt_client
        .save_prompt(prompt_name, "test prompt", prompt_text)
        .await?;

    // Associate prompt with the AI integration
    integration_client
        .associate_prompt_with_integration(&llm_provider, text_complete_model, prompt_name)
        .await?;

    // Test the prompt
    let mut test_vars = HashMap::new();
    test_vars.insert("friend_name".to_string(), serde_json::json!("Orkes"));

    let test_result = prompt_client
        .test_prompt(
            prompt_text,
            &test_vars,
            &llm_provider,
            text_complete_model,
            0.7,
            1.0,
            None,
        )
        .await?;

    println!("Test prompt result: {}", test_result);

    // Create the workflow: get_friend_name -> LLM text complete
    let get_name_task = WorkflowTask::simple("get_friends_name", "get_friend_name_ref");

    let text_complete_task = WorkflowTask::llm_text_complete(
        "say_hi_ref",
        &llm_provider,
        text_complete_model,
        prompt_name,
    )
    .with_prompt_variable(
        "friend_name",
        serde_json::json!("${get_friend_name_ref.output.result}"),
    );

    let workflow_def = WorkflowDef::new("say_hi_to_the_friend")
        .with_description("LLM Chain: Get name and generate greeting")
        .with_task(get_name_task)
        .with_task(text_complete_task)
        .with_output_param("greetings", "${say_hi_ref.output.result}");

    // Register the workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);

    // Execute the workflow
    let request = StartWorkflowRequest::new(&workflow_def.name);

    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await?;

    println!("\nWorkflow completed with status: {:?}", result.status);
    println!("Output: {}", serde_json::to_string_pretty(&result.output)?);

    // Cleanup
    handler.stop().await?;

    Ok(())
}
