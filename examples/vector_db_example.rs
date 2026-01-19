//! Vector Database Example
//!
//! This example demonstrates how to use vector database tasks for RAG workflows.
//! It mirrors the Python SDK's orkes/vector_db_helloworld.py example.
//!
//! Prerequisites:
//! - Conductor server running with OpenAI and Pinecone integrations configured
//! - CONDUCTOR_SERVER_URL environment variable set

use std::collections::HashMap;
use std::time::Duration;

use conductor::{
    ChatMessage, Configuration, OrkesClients, StartWorkflowRequest, WorkflowDef, WorkflowTask,
};

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
    let vector_db = format!("pinecone_{}", get_username());
    let embedding_model = "text-embedding-ada-002";
    let chat_model = "gpt-4";

    // Create clients
    let config = Configuration::default();
    let clients = OrkesClients::new(config)?;
    let workflow_client = clients.get_workflow_client();
    let metadata_client = clients.get_metadata_client();
    let prompt_client = clients.get_prompt_client();
    let integration_client = clients.get_integration_client();

    // Define QA prompt
    let prompt_name = "us_constitution_qna";
    let prompt_text = r#"
    Here is the fragment of the us constitution ${text}.  
    I have a question ${question}.
    Given the text fragment from the constitution - please answer the question. 
    If you cannot answer from within this context of text then say I don't know.
    "#;

    prompt_client
        .save_prompt(
            prompt_name,
            "US Constitution QnA",
            prompt_text,
        )
        .await?;

    integration_client
        .associate_prompt_with_integration(&llm_provider, chat_model, prompt_name)
        .await?;

    // Build the RAG workflow:
    // 1. Search the vector index for relevant text
    // 2. Use LLM to answer the question with context

    let question = "what is the first amendment to the constitution?";

    // Search the vector DB for relevant content
    let search_index =
        WorkflowTask::llm_search_index("search_vectordb", &vector_db, "test", question)
            .with_namespace("us_constitution")
            .with_max_results(2)
            .with_embedding_model(&llm_provider, embedding_model);

    // Use chat complete to answer with context
    let chat_complete =
        WorkflowTask::llm_chat_complete("chat_complete_ref", &llm_provider, chat_model)
            .with_instructions_template(prompt_name)
            .with_messages(vec![ChatMessage::user(question)])
            .with_prompt_variable(
                "text",
                serde_json::json!("${search_vectordb.output.result..text}"),
            )
            .with_prompt_variable("question", serde_json::json!(question));

    // Create workflow
    let workflow_def = WorkflowDef::new("vector_db_rag_example")
        .with_description("RAG workflow: Search vector DB and answer question")
        .with_version(1)
        .with_task(search_index)
        .with_task(chat_complete)
        .with_output_param("answer", "${chat_complete_ref.output.result}")
        .with_output_param("sources", "${search_vectordb.output.result}");

    // Register workflow
    metadata_client
        .register_or_update_workflow_def(&workflow_def, true)
        .await?;

    println!("Workflow registered: {}", workflow_def.name);

    // Execute the workflow
    let request = StartWorkflowRequest::new(&workflow_def.name).with_version(1);

    println!("\nAsking: {}", question);
    println!("Searching vector database and generating answer...\n");

    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(30))
        .await?;

    println!("Workflow status: {:?}", result.status);
    println!("\nAnswer:");
    if let Some(answer) = result.output.get("answer") {
        println!("{}", answer);
    }

    Ok(())
}

/// Example: Indexing documents to the vector database
/// This would typically be done separately before running RAG queries
#[allow(dead_code)]
async fn index_document_example(
    clients: &OrkesClients,
    llm_provider: &str,
    vector_db: &str,
    embedding_model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata_client = clients.get_metadata_client();
    let workflow_client = clients.get_workflow_client();

    // Index a document from URL
    let index_doc = WorkflowTask::llm_index_document(
        "index_doc_ref",
        vector_db,
        "test",
        "https://constitutioncenter.org/media/files/constitution.pdf",
        "us_constitution",
    )
    .with_namespace("us_constitution")
    .with_media_type("application/pdf")
    .with_embedding_model(llm_provider, embedding_model)
    .with_metadata({
        let mut m = HashMap::new();
        m.insert("source".to_string(), "constitution center".to_string());
        m
    });

    let index_workflow = WorkflowDef::new("index_document_workflow").with_task(index_doc);

    metadata_client
        .register_or_update_workflow_def(&index_workflow, true)
        .await?;

    // Execute indexing
    let request = StartWorkflowRequest::new("index_document_workflow");
    let result = workflow_client
        .execute_workflow(&request, Duration::from_secs(120))
        .await?;

    println!("Indexing completed: {:?}", result.status);

    Ok(())
}
