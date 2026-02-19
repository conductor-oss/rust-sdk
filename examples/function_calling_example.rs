// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{ChatMessage, StartWorkflowRequest, WorkflowDef, WorkflowTask, WorkflowTimeoutPolicy},
};

// Configuration
const LLM_PROVIDER: &str = "openai";
const LLM_MODEL: &str = "gpt-4o-mini";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("LLM Function Calling Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    // ==========================================================================
    // Example 1: Intent Classification
    // ==========================================================================
    println!("\nEXAMPLE 1: INTENT CLASSIFICATION");
    println!("{}", "=".repeat(80));
    println!();

    let workflow_name = "rust_intent_classifier";

    // Define functions for intent classification
    let intent_functions = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "handle_order_inquiry",
                "description": "Handle questions about orders, shipping, or delivery status",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "order_id": {
                            "type": "string",
                            "description": "Order ID if mentioned"
                        },
                        "inquiry_type": {
                            "type": "string",
                            "enum": ["status", "tracking", "cancel", "modify"],
                            "description": "Type of order inquiry"
                        }
                    },
                    "required": ["inquiry_type"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "handle_product_question",
                "description": "Handle questions about products, pricing, or availability",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Name of the product being asked about"
                        },
                        "question_type": {
                            "type": "string",
                            "enum": ["price", "availability", "features", "comparison"],
                            "description": "Type of product question"
                        }
                    },
                    "required": ["question_type"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "handle_support_request",
                "description": "Handle technical support or complaint issues",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "issue_type": {
                            "type": "string",
                            "enum": ["technical", "billing", "complaint", "feedback"],
                            "description": "Type of support issue"
                        },
                        "urgency": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "description": "Urgency level of the issue"
                        }
                    },
                    "required": ["issue_type"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "handle_general_inquiry",
                "description": "Handle general questions not fitting other categories",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "topic": {
                            "type": "string",
                            "description": "General topic of the inquiry"
                        }
                    },
                    "required": ["topic"]
                }
            }
        }
    ]);

    let system_prompt = r#"You are a customer service routing assistant.
Analyze the customer's message and classify their intent by calling the appropriate function.
Always call exactly one function that best matches the customer's needs."#;

    let classify_task =
        WorkflowTask::llm_chat_complete("classify_intent_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(system_prompt),
                ChatMessage::user("${workflow.input.customer_message}"),
            ])
            .with_input_param("tools", intent_functions)
            .with_input_param("tool_choice", "required")
            .with_temperature(0.0);

    // Parse and format the result
    let parse_script = r#"
    (function(){
        var output = $.llm_output;
        var result = {
            raw_response: output,
            intent: null,
            parameters: null
        };
        
        // Extract function call
        if (output.tool_calls && output.tool_calls.length > 0) {
            var call = output.tool_calls[0];
            result.intent = call.function.name;
            result.parameters = typeof call.function.arguments === 'string' 
                ? JSON.parse(call.function.arguments) 
                : call.function.arguments;
        } else if (output.function_call) {
            result.intent = output.function_call.name;
            result.parameters = typeof output.function_call.arguments === 'string'
                ? JSON.parse(output.function_call.arguments)
                : output.function_call.arguments;
        }
        
        return result;
    })();
    "#;

    let parse_task = WorkflowTask::inline("parse_intent_ref", parse_script)
        .with_input_param("llm_output", "${classify_intent_ref.output}");

    let workflow = WorkflowDef::new(workflow_name)
        .with_description("Classify customer intent using LLM function calling")
        .with_version(1)
        .with_task(classify_task)
        .with_task(parse_task)
        .with_input_parameters(vec!["customer_message".to_string()])
        .with_output_param("intent", "${parse_intent_ref.output.result.intent}")
        .with_output_param("parameters", "${parse_intent_ref.output.result.parameters}")
        .with_timeout(60, WorkflowTimeoutPolicy::TimeOutWf);

    println!("Workflow: {}", workflow.name);
    println!("Use Case: Route customer messages to appropriate handlers");
    println!();
    println!("Available Intents:");
    println!("  - handle_order_inquiry    (orders, shipping, delivery)");
    println!("  - handle_product_question (products, pricing, availability)");
    println!("  - handle_support_request  (support, complaints, feedback)");
    println!("  - handle_general_inquiry  (general questions)");
    println!();

    metadata_client
        .register_or_update_workflow_def(&workflow, true)
        .await?;
    println!("Workflow registered: {}", workflow_name);

    // Test with example messages
    let test_messages = vec![
        "Where is my order #12345?",
        "How much does the premium subscription cost?",
        "My account keeps getting locked and I need urgent help!",
        "What are your business hours?",
    ];

    println!("\n{}", "-".repeat(60));
    println!("TEST CLASSIFICATIONS");
    println!("{}", "-".repeat(60));

    for message in test_messages {
        println!("\nMessage: \"{}\"", message);

        let request = StartWorkflowRequest::new(workflow_name)
            .with_version(1)
            .with_input_value("customer_message", message);

        match workflow_client.start_workflow(&request).await {
            Ok(id) => {
                // Wait for completion
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Ok(wf) = workflow_client.get_workflow(&id, false).await {
                    if let Some(intent) = wf.output.get("intent") {
                        println!("  → Intent: {}", intent);
                    }
                    if let Some(params) = wf.output.get("parameters") {
                        println!(
                            "  → Params: {}",
                            serde_json::to_string(params).unwrap_or_default()
                        );
                    }
                }
            }
            Err(e) => println!("  → Could not classify: {}", e),
        }
    }

    // ==========================================================================
    // Example 2: Parameter Extraction
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("EXAMPLE 2: PARAMETER EXTRACTION");
    println!("{}", "=".repeat(80));
    println!();

    let extraction_workflow = "rust_param_extractor";

    // Define function for booking extraction
    let booking_function = serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "create_booking",
                "description": "Create a booking with the extracted parameters",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "check_in_date": {
                            "type": "string",
                            "description": "Check-in date in YYYY-MM-DD format"
                        },
                        "check_out_date": {
                            "type": "string",
                            "description": "Check-out date in YYYY-MM-DD format"
                        },
                        "num_guests": {
                            "type": "integer",
                            "description": "Number of guests"
                        },
                        "room_type": {
                            "type": "string",
                            "enum": ["standard", "deluxe", "suite"],
                            "description": "Type of room"
                        },
                        "special_requests": {
                            "type": "string",
                            "description": "Any special requests from the guest"
                        }
                    },
                    "required": ["check_in_date", "check_out_date", "num_guests"]
                }
            }
        }
    ]);

    let extraction_prompt = r#"You are a booking assistant. Extract booking parameters from the user's request.
Today's date is 2024-01-15. Convert relative dates (like "next Friday") to YYYY-MM-DD format.
If information is not provided, make reasonable assumptions based on context."#;

    let extract_task =
        WorkflowTask::llm_chat_complete("extract_params_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(extraction_prompt),
                ChatMessage::user("${workflow.input.booking_request}"),
            ])
            .with_input_param("tools", booking_function)
            .with_input_param(
                "tool_choice",
                serde_json::json!({"type": "function", "function": {"name": "create_booking"}}),
            )
            .with_temperature(0.0);

    let format_script = r#"
    (function(){
        var output = $.llm_output;
        var booking = null;
        
        if (output.tool_calls && output.tool_calls.length > 0) {
            var args = output.tool_calls[0].function.arguments;
            booking = typeof args === 'string' ? JSON.parse(args) : args;
        } else if (output.function_call) {
            var args = output.function_call.arguments;
            booking = typeof args === 'string' ? JSON.parse(args) : args;
        }
        
        return {
            booking: booking,
            is_complete: booking && booking.check_in_date && booking.check_out_date && booking.num_guests
        };
    })();
    "#;

    let format_task = WorkflowTask::inline("format_booking_ref", format_script)
        .with_input_param("llm_output", "${extract_params_ref.output}");

    let extraction_wf = WorkflowDef::new(extraction_workflow)
        .with_description("Extract structured booking parameters from natural language")
        .with_version(1)
        .with_task(extract_task)
        .with_task(format_task)
        .with_input_parameters(vec!["booking_request".to_string()])
        .with_output_param("booking", "${format_booking_ref.output.result.booking}")
        .with_output_param(
            "is_complete",
            "${format_booking_ref.output.result.is_complete}",
        )
        .with_timeout(60, WorkflowTimeoutPolicy::TimeOutWf);

    println!("Workflow: {}", extraction_wf.name);
    println!("Use Case: Extract structured data from natural language requests");
    println!();

    metadata_client
        .register_or_update_workflow_def(&extraction_wf, true)
        .await?;
    println!("Workflow registered: {}", extraction_workflow);

    // Test extraction
    let booking_request =
        "I'd like to book a deluxe room for 2 guests from January 20th to January 25th. We need a late checkout if possible.";

    println!("\n{}", "-".repeat(60));
    println!("TEST EXTRACTION");
    println!("{}", "-".repeat(60));
    println!("\nRequest: \"{}\"", booking_request);

    let request = StartWorkflowRequest::new(extraction_workflow)
        .with_version(1)
        .with_input_value("booking_request", booking_request);

    match workflow_client.start_workflow(&request).await {
        Ok(id) => {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Ok(wf) = workflow_client.get_workflow(&id, false).await {
                if let Some(booking) = wf.output.get("booking") {
                    println!(
                        "\nExtracted Booking:\n{}",
                        serde_json::to_string_pretty(booking)?
                    );
                }
            }
        }
        Err(e) => println!("  Could not extract: {}", e),
    }

    // ==========================================================================
    // Cleanup
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CLEANUP");
    println!("{}", "=".repeat(80));
    println!();

    for name in [workflow_name, extraction_workflow] {
        match metadata_client.delete_workflow_def(name, 1).await {
            Ok(_) => println!("  Deleted workflow: {}", name),
            Err(e) => println!("  Could not delete {}: {}", name, e),
        }
    }

    println!();
    println!("Function calling example completed!");
    println!();
    println!("Key Concepts:");
    println!("  1. Define functions using OpenAI-compatible tool schema");
    println!("  2. Use `tools` input parameter to pass function definitions");
    println!("  3. Use `tool_choice` to control function selection:");
    println!("     - \"auto\"     - LLM decides whether to call a function");
    println!("     - \"required\" - LLM must call a function");
    println!("     - {{\"type\": \"function\", \"function\": {{\"name\": \"...\"}}}}");
    println!("                   - Force specific function");
    println!("  4. Parse tool_calls from LLM response for routing/processing");

    Ok(())
}
