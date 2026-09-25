// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

#[path = "support/mod.rs"]
mod support;

use conductor::agents::{AgentDef, Strategy, SwarmTransition};
use conductor::configuration::Configuration;
use conductor::error::Result;
use serde_json::Value;
use support::run_with_local_tools;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Configuration::from_env();

    let backend_dev = AgentDef::new("backend_dev")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are a backend developer. You design APIs, databases, and server \
         architecture. Provide technical recommendations with code examples.",
        );

    let frontend_dev = AgentDef::new("frontend_dev")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are a frontend developer. You design UI components, user flows, \
         and client-side architecture. Provide recommendations with code examples.",
        );

    let content_writer = AgentDef::new("content_writer")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are a content writer. You create blog posts, landing page copy, \
         and marketing materials. Write engaging, clear content.",
        );

    let seo_specialist = AgentDef::new("seo_specialist")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are an SEO specialist. You optimize content for search engines, \
         suggest keywords, and improve page rankings.",
        );

    let engineering_lead = AgentDef::new("engineering_lead")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are the engineering lead. Route technical questions to the right \
             specialist: backend_dev for APIs/databases/servers, \
             frontend_dev for UI/UX/client-side.",
        )
        .with_sub_agent(backend_dev)?
        .with_sub_agent(frontend_dev)?
        .with_strategy(Strategy::Handoff)?;

    let marketing_lead = AgentDef::new("marketing_lead")?
        .with_model(support::llm_model())
        .with_instructions(
            "You are the marketing lead. Route marketing questions to the right \
             specialist: content_writer for blog posts/copy, \
             seo_specialist for SEO/keywords/rankings.",
        )
        .with_sub_agent(content_writer)?
        .with_sub_agent(seo_specialist)?
        .with_strategy(Strategy::Handoff)?;

    let ceo = AgentDef::new("ceo")?
        .with_model(support::llm_model())
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

    let result = run_with_local_tools(
        &config,
        &ceo,
        Value::String(
            "Design a REST API for a user management system with authentication, \
             then ask the marketing team for a campaign to promote it."
                .into(),
        ),
    )
    .await?;

    println!("status: {}", result.status);
    println!("output: {}", result.output);
    Ok(())
}
