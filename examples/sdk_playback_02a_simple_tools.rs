//! Reproduces `llm-recordings/02a_simple_tools` (from conductor-oss/conductor PR #1614).
//! Ported field-for-field from python-sdk's `examples/agents/02a_simple_tools.py`, including
//! `temperature=0.0` — the recorded requests carry it explicitly, so it must be set here too or
//! the mock provider's exact-request match fails.
//!
//! `cargo run --features agents --example sdk_playback_02a_simple_tools`

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, ToolDef};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use support::run_with_local_tools;

#[derive(Deserialize)]
struct CityArgs {
    city: String,
}

#[derive(Deserialize)]
struct SymbolArgs {
    symbol: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let get_weather = ToolDef::function(
        "get_weather",
        "Get the current weather for a city.",
        json!({
            "type": "object",
            "properties": { "city": { "type": "string" } },
            "required": ["city"]
        }),
        |args: CityArgs| async move {
            Ok(json!({ "city": args.city, "temp_f": 72, "condition": "Sunny" }))
        },
    );

    let get_stock_price = ToolDef::function(
        "get_stock_price",
        "Get the current stock price for a ticker symbol.",
        json!({
            "type": "object",
            "properties": { "symbol": { "type": "string" } },
            "required": ["symbol"]
        }),
        |args: SymbolArgs| async move {
            Ok(json!({ "symbol": args.symbol, "price": 182.50, "change": "+1.2%" }))
        },
    );

    let agent = AgentDef::new("weather_stock_agent")?
        .with_model("mock/mockLLM")
        .with_temperature(0.0)
        .with_instructions("You are a helpful assistant. Use tools to answer questions.")
        .with_tool(get_weather)
        .with_tool(get_stock_price);

    let result = run_with_local_tools(
        &config,
        &agent,
        Value::String("What's the weather like in San Francisco?".into()),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
