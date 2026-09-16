//! Reproduces `llm-recordings/06_sequential_pipeline` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/06_sequential_pipeline.py`'s
//! "Option 2" shape (`agents=[...], strategy=Strategy.SEQUENTIAL)`) — Rust has no `>>` operator
//! equivalent to python's "Option 1", but both compile to the same wire shape.
//!
//! `cargo run --features agents --example sdk_playback_06_sequential_pipeline`

use conductor::agents::{AgentDef, AgentRuntime, Strategy};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();
    let runtime = AgentRuntime::new(config)?;

    let researcher = AgentDef::new("researcher")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a researcher. Given a topic, provide key facts and data points. \
             Be thorough but concise. Output raw research findings.",
        );

    let writer = AgentDef::new("writer")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are a writer. Take research findings and write a clear, engaging \
             article. Use headers and bullet points where appropriate.",
        );

    let editor = AgentDef::new("editor")?
        .with_model("mock/mockLLM")
        .with_instructions(
            "You are an editor. Review the article for clarity, grammar, and tone. \
             Make improvements and output the final polished version.",
        );

    let pipeline = AgentDef::new("content_pipeline")?
        .with_sub_agent(researcher)?
        .with_sub_agent(writer)?
        .with_sub_agent(editor)?
        .with_strategy(Strategy::Sequential)?;

    let result = runtime
        .run(
            &pipeline,
            Value::String("The impact of AI agents on software development in 2025".into()),
        )
        .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
