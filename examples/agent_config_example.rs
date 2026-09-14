// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! Local-only smoke test for the `agents` feature: builds a `#[tool]`, attaches it to a
//! sub-agent, composes that into a coordinator agent, and prints the serialized `agentConfig`
//! JSON. No server connection required.

use conductor::agents::{AgentConfigSerializer, AgentDef, Strategy};
use conductor::error::Result;
use conductor_macros::tool;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize, JsonSchema)]
struct GetWeatherArgs {
    city: String,
}

#[tool(description = "Get current weather for a city")]
async fn get_weather(args: GetWeatherArgs) -> Result<Value> {
    Ok(serde_json::json!({ "city": args.city, "forecast": "sunny" }))
}

fn main() -> Result<()> {
    let researcher = AgentDef::new("researcher")?
        .with_model("gpt-4o")
        .with_instructions("Research weather conditions using the get_weather tool.")
        .with_tool(get_weather_tool());

    let coordinator = AgentDef::new("coordinator")?
        .with_instructions("Delegate weather questions to the researcher agent.")
        .with_sub_agent(researcher)?
        .with_strategy(Strategy::Sequential)?;

    let json = AgentConfigSerializer::serialize(&coordinator);
    println!("{}", serde_json::to_string_pretty(&json)?);

    Ok(())
}
