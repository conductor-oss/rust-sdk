//! One-off probe: confirms the server accepts each of the 8 new tool-type wire shapes via
//! /agent/compile. Not part of the crate's example set.

use conductor::agents::{AgentDef, AgentRuntime, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use std::collections::HashMap;

async fn try_compile(runtime: &AgentRuntime, label: &str, agent: &AgentDef) {
    print!("{label}: ");
    match runtime.compile(agent).await {
        Ok(v) => {
            let workers = v.get("requiredWorkers").cloned().unwrap_or_default();
            println!("OK (requiredWorkers={workers})");
        }
        Err(e) => println!("ERROR: {e}"),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let wrap = |tool: ToolDef| -> Result<AgentDef> {
        Ok(AgentDef::new(format!("probe-{}", tool.name))?
            .with_model("mock/mockLLM")
            .with_tool(tool))
    };

    try_compile(
        &runtime,
        "api",
        &wrap(ToolDef::api(
            "https://api.example.com/openapi.json",
            "example_api",
            "Example API tools",
            HashMap::new(),
            None,
            64,
            vec![],
        )?)?,
    )
    .await;

    try_compile(
        &runtime,
        "image",
        &wrap(ToolDef::image(
            "generate_image",
            "Generate an image",
            "openai",
            "dall-e-3",
            None,
            HashMap::new(),
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "audio",
        &wrap(ToolDef::audio(
            "text_to_speech",
            "Convert text to speech",
            "openai",
            "tts-1",
            None,
            HashMap::new(),
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "video",
        &wrap(ToolDef::video(
            "generate_video",
            "Generate a video",
            "openai",
            "sora-2",
            None,
            HashMap::new(),
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "pdf",
        &wrap(ToolDef::pdf(
            "generate_pdf",
            "Generate a PDF document from markdown text.",
            None,
            HashMap::new(),
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "rag_index",
        &wrap(ToolDef::rag_index(
            "index_document",
            "Index a document",
            "pgvectordb",
            "product_docs",
            "openai",
            "text-embedding-3-small",
            None,
            None,
            None,
            None,
            None,
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "rag_search",
        &wrap(ToolDef::rag_search(
            "search_kb",
            "Search the knowledge base",
            "pgvectordb",
            "product_docs",
            "openai",
            "text-embedding-3-small",
            None,
            None,
            None,
            None,
        ))?,
    )
    .await;

    try_compile(
        &runtime,
        "wait_for_message",
        &wrap(ToolDef::wait_for_message(
            "wait_for_message",
            "Wait for a message",
            1,
            true,
        ))?,
    )
    .await;

    Ok(())
}
