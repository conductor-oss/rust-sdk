// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{ChatMessage, StartWorkflowRequest, WorkflowDef, WorkflowStatus, WorkflowTask, WorkflowTimeoutPolicy},
};
use std::collections::HashMap;

// Configuration constants
const VECTOR_DB: &str = "postgres-prod";
const VECTOR_INDEX: &str = "demo_index";
const EMBEDDING_PROVIDER: &str = "openai";
const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const EMBEDDING_DIMENSIONS: i32 = 1536;
const LLM_PROVIDER: &str = "openai";
const LLM_MODEL: &str = "gpt-4o-mini";
const NAMESPACE: &str = "demo_namespace";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("RAG Workflow Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config.clone())?;
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    // ==========================================================================
    // Create the RAG Pipeline Workflow
    // ==========================================================================
    println!("\nRAG PIPELINE WORKFLOW");
    println!("{}", "=".repeat(80));
    println!();

    let workflow_name = "rust_rag_pipeline";

    // Step 1: Index text into vector database
    let index_task = WorkflowTask::llm_index_text(
        "index_text_ref",
        VECTOR_DB,
        VECTOR_INDEX,
        "${workflow.input.text}",
        "${workflow.input.doc_id}",
    )
    .with_namespace(NAMESPACE)
    .with_embedding_model(EMBEDDING_PROVIDER, EMBEDDING_MODEL)
    .with_input_param("dimensions", EMBEDDING_DIMENSIONS)
    .with_input_param("chunkSize", 1024)
    .with_input_param("chunkOverlap", 128)
    .with_metadata(HashMap::from([
        ("source".to_string(), "${workflow.input.source}".to_string()),
        ("title".to_string(), "${workflow.input.title}".to_string()),
    ]));

    // Step 2: Wait for vector DB to commit (eventual consistency)
    let wait_task = WorkflowTask::wait_duration("wait_for_index_ref", "5s")
        .with_description("Wait for vector DB to commit the embeddings");

    // Step 3: Search the index with user's question
    let search_task = WorkflowTask::llm_search_index(
        "search_index_ref",
        VECTOR_DB,
        VECTOR_INDEX,
        "${workflow.input.question}",
    )
    .with_namespace(NAMESPACE)
    .with_embedding_model(EMBEDDING_PROVIDER, EMBEDDING_MODEL)
    .with_max_results(5)
    .with_input_param("dimensions", EMBEDDING_DIMENSIONS);

    // Step 4: Generate answer using retrieved context
    let answer_task = WorkflowTask::llm_chat_complete("generate_answer_ref", LLM_PROVIDER, LLM_MODEL)
        .with_messages(vec![
            ChatMessage::system(
                "You are a helpful assistant. Answer the user's question \
                based ONLY on the context provided below. If the context \
                does not contain enough information, say so.\n\n\
                Context from knowledge base:\n\
                ${search_index_ref.output.result}",
            ),
            ChatMessage::user("${workflow.input.question}"),
        ])
        .with_temperature(0.2)
        .with_max_tokens(1024);

    // Build the workflow definition
    let workflow = WorkflowDef::new(workflow_name)
        .with_description("RAG pipeline: index text -> search -> answer")
        .with_version(1)
        .with_task(index_task)
        .with_task(wait_task)
        .with_task(search_task)
        .with_task(answer_task)
        .with_input_parameters(vec![
            "text".to_string(),
            "doc_id".to_string(),
            "source".to_string(),
            "title".to_string(),
            "question".to_string(),
        ])
        .with_output_param("indexing_status", "${index_text_ref.output}")
        .with_output_param("retrieved_context", "${search_index_ref.output.result}")
        .with_output_param("final_answer", "${generate_answer_ref.output.result}")
        .with_timeout(600, WorkflowTimeoutPolicy::TimeOutWf);

    println!("Workflow: {}", workflow.name);
    println!("Description: {:?}", workflow.description);
    println!();
    println!("Pipeline:");
    println!("  1. index_text_ref     - Index text into vector DB");
    println!("  2. wait_for_index_ref - Wait 5s for DB commit");
    println!("  3. search_index_ref   - Search for relevant context");
    println!("  4. generate_answer_ref - Generate answer with LLM");
    println!();

    // Register the workflow
    println!("Registering workflow...");
    metadata_client.register_or_update_workflow_def(&workflow, true).await?;
    println!("  Workflow registered: {}", workflow_name);

    // ==========================================================================
    // Display Configuration Details
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CONFIGURATION DETAILS");
    println!("{}", "=".repeat(80));
    println!();
    println!("Vector Database:");
    println!("  Integration: {}", VECTOR_DB);
    println!("  Index: {}", VECTOR_INDEX);
    println!("  Namespace: {}", NAMESPACE);
    println!();
    println!("Embeddings:");
    println!("  Provider: {}", EMBEDDING_PROVIDER);
    println!("  Model: {}", EMBEDDING_MODEL);
    println!("  Dimensions: {}", EMBEDDING_DIMENSIONS);
    println!();
    println!("LLM:");
    println!("  Provider: {}", LLM_PROVIDER);
    println!("  Model: {}", LLM_MODEL);
    println!();

    // ==========================================================================
    // Example Execution (if integrations are configured)
    // ==========================================================================
    println!("{}", "=".repeat(80));
    println!("EXAMPLE EXECUTION");
    println!("{}", "=".repeat(80));
    println!();

    // Sample document to index
    let sample_text = r#"
Conductor is a workflow orchestration engine that helps you manage complex workflows.
Key features include:
- Workflow as code: Define workflows in JSON or using SDKs
- Task workers: Implement task logic in any language
- Visual workflow editor: Create and modify workflows visually
- Built-in retry and error handling: Automatic retries with backoff
- Scalability: Handle millions of concurrent workflows
- Observability: Full visibility into workflow execution

Conductor supports various task types including HTTP, Lambda, Kafka, and custom workers.
It's used by companies like Netflix, Orkes, and many others for workflow orchestration.
"#;

    println!("Sample Input:");
    println!("  Document: {} chars", sample_text.len());
    println!("  Doc ID: conductor_overview");
    println!("  Question: What are the key features of Conductor?");
    println!();

    println!("Starting workflow execution...");
    
    // Build the start request
    let request = StartWorkflowRequest::new(workflow_name)
        .with_version(1)
        .with_input_value("text", sample_text)
        .with_input_value("doc_id", "conductor_overview")
        .with_input_value("source", "documentation")
        .with_input_value("title", "Conductor Overview")
        .with_input_value("question", "What are the key features of Conductor?");
    
    match workflow_client.start_workflow(&request).await {
        Ok(workflow_id) => {
            println!("  Workflow started: {}", workflow_id);
            println!();
            println!("  View execution at:");
            
            // Extract UI host from config
            let server_url = std::env::var("CONDUCTOR_SERVER_URL")
                .unwrap_or_else(|_| "http://localhost:8080/api".to_string());
            let ui_url = server_url.replace("/api", "");
            println!("  {}/execution/{}", ui_url, workflow_id);
            println!();

            // Poll for completion
            println!("Waiting for completion (timeout: 60s)...");
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(60);

            loop {
                if start.elapsed() > timeout {
                    println!("  Timeout waiting for workflow completion");
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                match workflow_client.get_workflow(&workflow_id, true).await {
                    Ok(wf) => {
                        let status = wf.status;
                        print!("\r  Status: {:?}    ", status);

                        if status == WorkflowStatus::Completed {
                            println!();
                            println!();
                            println!("{}", "=".repeat(80));
                            println!("RESULTS");
                            println!("{}", "=".repeat(80));
                            println!();

                            // Retrieved context
                            if let Some(context) = wf.output.get("retrieved_context") {
                                println!("Retrieved Context:");
                                if let Some(arr) = context.as_array() {
                                    println!("  {} chunk(s) found", arr.len());
                                    for (i, chunk) in arr.iter().take(3).enumerate() {
                                        if let Some(text) = chunk.get("text") {
                                            let preview = text.as_str().unwrap_or("").chars().take(100).collect::<String>();
                                            println!("  {}. {}...", i + 1, preview);
                                        }
                                    }
                                }
                                println!();
                            }

                            // Final answer
                            if let Some(answer) = wf.output.get("final_answer") {
                                println!("Generated Answer:");
                                println!("{}", "-".repeat(60));
                                println!("{}", answer.as_str().unwrap_or("No answer"));
                                println!("{}", "-".repeat(60));
                            }
                            break;
                        } else if matches!(status, WorkflowStatus::Failed | WorkflowStatus::Terminated | WorkflowStatus::TimedOut)
                        {
                            println!();
                            println!("  Workflow failed: {:?}", status);
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
            println!("  Could not start workflow: {}", e);
            println!();
            println!("  This is expected if AI integrations are not configured.");
            println!("  Configure the following in your Conductor server:");
            println!("    - Vector DB: {} (e.g., pgvector)", VECTOR_DB);
            println!("    - LLM Provider: {} with API key", LLM_PROVIDER);
        }
    }

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
    println!("RAG workflow example completed!");
    println!();
    println!("Next Steps:");
    println!("  1. Configure vector DB integration (pgvector, pinecone, etc.)");
    println!("  2. Configure LLM provider (OpenAI, Anthropic, etc.)");
    println!("  3. Index your documents using the workflow");
    println!("  4. Query the knowledge base with natural language questions");

    Ok(())
}
