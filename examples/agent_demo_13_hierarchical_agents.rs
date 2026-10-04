// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::agents::AgentRuntime;
use conductor::agents::{AgentDef, Strategy, SwarmTransition};
use conductor::configuration::Configuration;
use conductor::error::{ConductorError, Result};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::args().nth(1).ok_or_else(|| {
        ConductorError::agent("Pass provider/model as the first argument after --")
    })?;
    if model.trim().is_empty() {
        return Err(ConductorError::agent("Model argument cannot be empty"));
    }
    let config = Configuration::from_env();

    let backend_dev = AgentDef::new("backend_dev")?
        .with_model(model.clone())
        .with_instructions(
            "You are a backend developer. You design APIs, databases, and server \
         architecture. Provide technical recommendations with code examples.",
        );

    let frontend_dev = AgentDef::new("frontend_dev")?
        .with_model(model.clone())
        .with_instructions(
            "You are a frontend developer. You design UI components, user flows, \
         and client-side architecture. Provide recommendations with code examples.",
        );

    let content_writer = AgentDef::new("content_writer")?
        .with_model(model.clone())
        .with_instructions(
            "You are a content writer. You create blog posts, landing page copy, \
         and marketing materials. Write engaging, clear content.",
        );

    let seo_specialist = AgentDef::new("seo_specialist")?
        .with_model(model.clone())
        .with_instructions(
            "You are an SEO specialist. You optimize content for search engines, \
         suggest keywords, and improve page rankings.",
        );

    let engineering_lead = AgentDef::new("engineering_lead")?
        .with_model(model.clone())
        .with_instructions(
            "You are the engineering lead. Route technical questions to the right \
             specialist: backend_dev for APIs/databases/servers, \
             frontend_dev for UI/UX/client-side.",
        )
        .with_sub_agent(backend_dev)?
        .with_sub_agent(frontend_dev)?
        .with_strategy(Strategy::Handoff)?;

    let marketing_lead = AgentDef::new("marketing_lead")?
        .with_model(model.clone())
        .with_instructions(
            "You are the marketing lead. Route marketing questions to the right \
             specialist: content_writer for blog posts/copy, \
             seo_specialist for SEO/keywords/rankings.",
        )
        .with_sub_agent(content_writer)?
        .with_sub_agent(seo_specialist)?
        .with_strategy(Strategy::Handoff)?;

    let ceo = AgentDef::new("ceo")?
        .with_model(model)
        .with_instructions(
            "You are the CEO. Route requests to the right department: \
             engineering_lead for technical/development questions, \
             marketing_lead for marketing/content/SEO questions.",
        )
        .with_sub_agent(engineering_lead)?
        .with_sub_agent(marketing_lead)?
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "engineering_lead".to_owned(),
            target: "engineering_lead".to_owned(),
        })
        .with_swarm_transition(SwarmTransition::OnTextMention {
            text: "marketing_lead".to_owned(),
            target: "marketing_lead".to_owned(),
        })
        .with_strategy(Strategy::Swarm)?;

    let mut runtime = AgentRuntime::new(config.clone())?;
    runtime.serve(&ceo).await?;
    let result = runtime
        .run(
            &ceo,
            Value::String(
                "Design a REST API for a user management system with authentication, \
             then ask the marketing team for a campaign to promote it."
                    .into(),
            ),
        )
        .await;
    runtime.shutdown().await?;
    let result = result?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
