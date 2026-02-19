// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::MetadataTag,
};
use std::collections::HashMap;
use std::env;

/// Track created resources for cleanup
struct PromptJourney {
    client: ConductorClient,
    created_prompts: Vec<String>,
    ai_integration: String,
}

impl PromptJourney {
    fn new() -> anyhow::Result<Self> {
        let config = Configuration::default();
        let client = ConductorClient::new(config)?;

        Ok(Self {
            client,
            created_prompts: Vec::new(),
            ai_integration: env::var("AI_INTEGRATION").unwrap_or_else(|_| "openai".to_string()),
        })
    }

    /// Chapter 1: Initial Setup - Creating Basic Prompt Templates
    async fn chapter1_initial_setup(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(60));
        println!(" CHAPTER 1: INITIAL SETUP");
        println!("{}", "=".repeat(60));
        println!("\nTechMart is launching AI-powered customer service.");
        println!("Let's create our first prompt templates...\n");

        let prompt_client = self.client.prompt_client();

        // Create greeting prompt
        println!("Creating customer greeting prompt...");
        let greeting_prompt = r#"You are a friendly customer service representative for TechMart.

Customer Name: ${customer_name}
Customer Tier: ${customer_tier}
Time of Day: ${time_of_day}

Greet the customer appropriately based on their tier and the time of day.
Keep the greeting warm, professional, and under 50 words."#;

        prompt_client
            .save_prompt(
                "rust_customer_greeting",
                "Personalized greeting for customers based on tier and time",
                greeting_prompt,
            )
            .await?;
        self.created_prompts.push("rust_customer_greeting".to_string());
        println!("  Created 'rust_customer_greeting' prompt");

        // Verify by retrieving
        println!("\nRetrieving the greeting prompt to verify...");
        let retrieved = prompt_client.get_prompt("rust_customer_greeting").await?;
        println!("  Name: {}", retrieved.name);
        println!("  Description: {:?}", retrieved.description);
        println!("  Variables: {:?}", retrieved.variables);

        // Create order inquiry prompt
        println!("\nCreating order inquiry prompt...");
        let order_prompt = r#"You are a helpful customer service agent for TechMart.

Customer Information:
- Name: ${customer_name}
- Order ID: ${order_id}
- Order Status: ${order_status}
- Delivery Date: ${delivery_date}

Customer Query: ${query}

Provide a clear, empathetic response about their order.
Include relevant details and next steps if applicable."#;

        prompt_client
            .save_prompt(
                "rust_order_inquiry",
                "Handle customer inquiries about order status",
                order_prompt,
            )
            .await?;
        self.created_prompts.push("rust_order_inquiry".to_string());
        println!("  Created 'rust_order_inquiry' prompt");

        // Create return request prompt
        println!("\nCreating return request prompt...");
        let return_prompt = r#"You are processing a return request for TechMart.

Product: ${product_name}
Purchase Date: ${purchase_date}
Reason: ${return_reason}
Condition: ${product_condition}

Return Policy: Items can be returned within 30 days in original condition.

Evaluate the return request and provide:
1. Whether the return is eligible
2. Next steps for the customer
3. Expected timeline

Be helpful and understanding while following company policy."#;

        prompt_client
            .save_prompt(
                "rust_return_request",
                "Process and respond to product return requests",
                return_prompt,
            )
            .await?;
        self.created_prompts.push("rust_return_request".to_string());
        println!("  Created 'rust_return_request' prompt");

        println!("\n  Chapter 1 Complete: Basic prompts created!");
        Ok(())
    }

    /// Chapter 2: Template Organization - Using Tags to Categorize Prompts
    async fn chapter2_template_organization(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(60));
        println!(" CHAPTER 2: TEMPLATE ORGANIZATION");
        println!("{}", "=".repeat(60));
        println!("\nOrganizing prompts with tags for better management...\n");

        let prompt_client = self.client.prompt_client();

        // Add tags to greeting prompt
        println!("Adding tags to customer greeting prompt...");
        let greeting_tags = vec![
            MetadataTag::with_value("category", "customer_service"),
            MetadataTag::with_value("type", "greeting"),
            MetadataTag::with_value("department", "support"),
            MetadataTag::with_value("language", "english"),
            MetadataTag::with_value("status", "active"),
            MetadataTag::with_value("priority", "high"),
        ];

        prompt_client
            .update_tag_for_prompt_template("rust_customer_greeting", &greeting_tags)
            .await?;
        println!("  Tags added to greeting prompt");

        // Verify tags
        println!("\nRetrieving tags for greeting prompt...");
        let retrieved_tags = prompt_client
            .get_tags_for_prompt_template("rust_customer_greeting")
            .await?;
        println!("  Tags ({} total):", retrieved_tags.len());
        for tag in &retrieved_tags {
            println!("    - {}: {}", tag.key, tag.value.as_deref().unwrap_or(""));
        }

        // Add tags to order inquiry prompt
        println!("\nAdding tags to order inquiry prompt...");
        let order_tags = vec![
            MetadataTag::with_value("category", "customer_service"),
            MetadataTag::with_value("type", "inquiry"),
            MetadataTag::with_value("department", "support"),
            MetadataTag::with_value("language", "english"),
            MetadataTag::with_value("status", "active"),
            MetadataTag::with_value("priority", "high"),
            MetadataTag::with_value("integration", "order_system"),
        ];

        prompt_client
            .update_tag_for_prompt_template("rust_order_inquiry", &order_tags)
            .await?;
        println!("  Tags added to order inquiry prompt");

        // Add tags to return request prompt
        println!("\nAdding tags to return request prompt...");
        let return_tags = vec![
            MetadataTag::with_value("category", "customer_service"),
            MetadataTag::with_value("type", "returns"),
            MetadataTag::with_value("department", "support"),
            MetadataTag::with_value("language", "english"),
            MetadataTag::with_value("status", "testing"),
            MetadataTag::with_value("priority", "medium"),
            MetadataTag::with_value("compliance", "requires_review"),
        ];

        prompt_client
            .update_tag_for_prompt_template("rust_return_request", &return_tags)
            .await?;
        println!("  Tags added to return request prompt");

        // Get all prompts and display by category
        println!("\nRetrieving all prompts organized by tags...");
        let all_prompts = prompt_client.get_prompts().await?;

        println!("\n  Prompts Summary:");
        for prompt in &all_prompts {
            if self.created_prompts.contains(&prompt.name) {
                println!("    - {} (models: {:?})", prompt.name, prompt.models);
            }
        }

        println!("\n  Chapter 2 Complete: Prompts organized with tags!");
        Ok(())
    }

    /// Chapter 3: Version Management - Creating and Managing Multiple Versions
    async fn chapter3_version_management(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(60));
        println!(" CHAPTER 3: VERSION MANAGEMENT");
        println!("{}", "=".repeat(60));
        println!("\nLearning to manage multiple versions of prompts...\n");

        let prompt_client = self.client.prompt_client();

        // Create FAQ prompt with explicit version 1
        println!("Creating FAQ response prompt - Version 1...");
        let faq_v1 = r#"Answer the customer's frequently asked question.

Question: ${question}

Provide a clear, concise answer."#;

        prompt_client
            .save_prompt_with_options(
                "rust_faq_response",
                "FAQ response generator - Initial version",
                faq_v1,
                None,
                Some(1),
                false,
            )
            .await?;
        self.created_prompts.push("rust_faq_response".to_string());
        println!("  Created FAQ response v1");

        // Create version 2 with improvements
        println!("\nCreating improved Version 2...");
        let faq_v2 = r#"You are a knowledgeable TechMart support agent answering FAQs.

Category: ${category}
Question: ${question}
Customer Type: ${customer_type}

Instructions:
- Provide accurate information
- Keep answer under 150 words
- Include relevant links if applicable
- Be friendly and helpful"#;

        prompt_client
            .save_prompt_with_options(
                "rust_faq_response",
                "FAQ response generator - Enhanced with category support",
                faq_v2,
                None,
                Some(2),
                false,
            )
            .await?;
        println!("  Created FAQ response v2 with category support");

        // Demonstrate auto-increment feature
        println!("\nUsing auto-increment for minor update...");
        let faq_v3 = r#"You are a knowledgeable TechMart support agent answering FAQs.

Category: ${category}
Question: ${question}
Customer Type: ${customer_type}
Urgency Level: ${urgency}

Instructions:
- Provide accurate information in a culturally appropriate manner
- Prioritize based on urgency level
- Keep answer under 150 words
- Include relevant links if applicable
- Be friendly and helpful"#;

        prompt_client
            .save_prompt_with_options(
                "rust_faq_response",
                "FAQ response generator - Added urgency handling",
                faq_v3,
                None,
                None,
                true, // auto_increment
            )
            .await?;
        println!("  Auto-incremented version with urgency handling");

        // Create prompt with model associations
        println!("\nCreating prompt with specific model associations...");
        let formal_greeting = r#"Dear ${customer_name},

Thank you for contacting TechMart support.

We appreciate your ${customer_tier} membership and are here to assist you.

How may we help you today?"#;

        let models = vec!["openai:gpt-4".to_string(), "openai:gpt-4o".to_string()];

        prompt_client
            .save_prompt_with_options(
                "rust_greeting_formal",
                "Formal greeting style for A/B testing",
                formal_greeting,
                Some(&models),
                Some(1),
                false,
            )
            .await?;
        self.created_prompts.push("rust_greeting_formal".to_string());
        println!("  Created formal greeting with model associations");

        // Tag versions for tracking
        println!("\nTagging versions for management...");
        let version_tags = vec![
            MetadataTag::with_value("version_status", "active"),
            MetadataTag::with_value("tested_models", "openai:gpt-4o"),
            MetadataTag::with_value("performance", "optimized"),
            MetadataTag::with_value("last_updated", "2024-12-24"),
        ];

        prompt_client
            .update_tag_for_prompt_template("rust_faq_response", &version_tags)
            .await?;
        println!("  Tagged FAQ response with version metadata");

        println!("\n  Version Management Best Practices:");
        println!("    1. Use explicit version numbers for major changes");
        println!("    2. Use auto-increment for minor updates");
        println!("    3. Tag versions with testing status and performance metrics");
        println!("    4. Specify compatible models for each version");

        println!("\n  Chapter 3 Complete: Version management mastered!");
        Ok(())
    }

    /// Chapter 4: Testing Prompts - Testing with AI models
    async fn chapter4_testing_prompts(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(60));
        println!(" CHAPTER 4: TESTING PROMPTS");
        println!("{}", "=".repeat(60));
        println!("\nTesting prompts with real data and AI models...\n");

        let prompt_client = self.client.prompt_client();

        // Get the greeting prompt template
        let greeting_template = prompt_client.get_prompt("rust_customer_greeting").await?;

        // Test with different scenarios
        println!("Testing customer greeting prompt...\n");

        let test_cases = vec![
            HashMap::from([
                ("customer_name".to_string(), serde_json::json!("John Smith")),
                ("customer_tier".to_string(), serde_json::json!("Premium")),
                ("time_of_day".to_string(), serde_json::json!("morning")),
            ]),
            HashMap::from([
                ("customer_name".to_string(), serde_json::json!("Sarah Johnson")),
                ("customer_tier".to_string(), serde_json::json!("Standard")),
                ("time_of_day".to_string(), serde_json::json!("evening")),
            ]),
        ];

        for (i, test_case) in test_cases.iter().enumerate() {
            println!("  Test Case {}:", i + 1);
            println!(
                "    Customer: {} ({})",
                test_case.get("customer_name").unwrap(),
                test_case.get("customer_tier").unwrap()
            );
            println!("    Time: {}", test_case.get("time_of_day").unwrap());

            match prompt_client
                .test_prompt(
                    &greeting_template.template,
                    test_case,
                    &self.ai_integration,
                    "gpt-4o",
                    0.7,
                    0.9,
                    None,
                )
                .await
            {
                Ok(response) => {
                    let preview = if response.len() > 200 {
                        format!("{}...", &response[..200])
                    } else {
                        response
                    };
                    println!("    Response: {}\n", preview);
                }
                Err(e) => {
                    println!("    Test skipped (AI integration required): {}\n", e);
                }
            }
        }

        // Test with different temperatures
        println!("Testing order inquiry with different creativity levels...");

        let order_template = prompt_client.get_prompt("rust_order_inquiry").await?;
        let order_test = HashMap::from([
            ("customer_name".to_string(), serde_json::json!("Alex Chen")),
            (
                "order_id".to_string(),
                serde_json::json!("ORD-2024-001234"),
            ),
            ("order_status".to_string(), serde_json::json!("In Transit")),
            (
                "delivery_date".to_string(),
                serde_json::json!("December 28, 2024"),
            ),
            (
                "query".to_string(),
                serde_json::json!("When will my order arrive? I need it for a gift."),
            ),
        ]);

        let temperature_tests = vec![
            ("Conservative", 0.3),
            ("Balanced", 0.7),
            ("Creative", 0.9),
        ];

        for (name, temp) in temperature_tests {
            println!("\n  Testing with {} temperature ({}):", name, temp);
            match prompt_client
                .test_prompt(
                    &order_template.template,
                    &order_test,
                    &self.ai_integration,
                    "gpt-4o",
                    temp,
                    0.9,
                    None,
                )
                .await
            {
                Ok(response) => {
                    let preview = if response.len() > 150 {
                        format!("{}...", &response[..150])
                    } else {
                        response
                    };
                    println!("    Response preview: {}", preview);
                }
                Err(e) => {
                    println!("    Test skipped (AI integration required): {}", e);
                }
            }
        }

        println!("\n  Chapter 4 Complete: Prompts tested and refined!");
        Ok(())
    }

    /// Chapter 5: Cleanup - Delete created resources
    async fn cleanup(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(60));
        println!(" CLEANUP");
        println!("{}", "=".repeat(60));
        println!("\nCleaning up created resources...\n");

        let prompt_client = self.client.prompt_client();

        for prompt_name in &self.created_prompts {
            match prompt_client.delete_prompt(prompt_name).await {
                Ok(_) => println!("  Deleted: {}", prompt_name),
                Err(e) => println!("  Could not delete {}: {}", prompt_name, e),
            }
        }

        println!("\n  Cleanup completed!");
        Ok(())
    }

    /// Run the complete journey
    async fn run(&mut self) -> anyhow::Result<()> {
        println!("\n{}", "=".repeat(80));
        println!(" PROMPT MANAGEMENT JOURNEY: AI-POWERED CUSTOMER SERVICE");
        println!("{}", "=".repeat(80));
        println!("\nWelcome to TechMart's journey to build an AI-powered customer service system!");
        println!("We'll explore all Prompt Management APIs through real-world scenarios.\n");

        self.chapter1_initial_setup().await?;
        self.chapter2_template_organization().await?;
        self.chapter3_version_management().await?;
        self.chapter4_testing_prompts().await?;

        println!("\n{}", "=".repeat(80));
        println!(" JOURNEY COMPLETED SUCCESSFULLY!");
        println!("{}", "=".repeat(80));
        println!("\nCongratulations! You've successfully explored all Prompt Management APIs.");
        println!("Your AI-powered customer service system is ready for production!\n");

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Prompt Management Journey - Conductor Rust SDK\n");

    let mut journey = PromptJourney::new()?;

    match journey.run().await {
        Ok(_) => {
            // Cleanup on success
            if let Err(e) = journey.cleanup().await {
                eprintln!("Cleanup warning: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Journey failed: {}", e);
            // Cleanup on failure
            if let Err(cleanup_err) = journey.cleanup().await {
                eprintln!("Cleanup warning: {}", cleanup_err);
            }
            return Err(e);
        }
    }

    Ok(())
}
