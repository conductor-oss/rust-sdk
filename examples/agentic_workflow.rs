//! Agentic Workflow Example
//!
//! This example demonstrates building an AI agent using Conductor workflows.
//! The agent can:
//! 1. Decide which tool to use based on user input
//! 2. Execute the selected tool (implemented as task workers)
//! 3. Return results to the user
//!
//! ## Agent Architecture
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                          AGENT WORKFLOW                             │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                     │
//! │  ┌──────────────┐    ┌──────────────┐    ┌───────────────────────┐ │
//! │  │  LLM Task    │───▶│  Switch/Case │───▶│  Tool Worker Tasks    │ │
//! │  │  (Reasoning) │    │  (Routing)   │    │  (get_weather, etc.)  │ │
//! │  └──────────────┘    └──────────────┘    └───────────────────────┘ │
//! │                                                                     │
//! │  Input: user_question ────▶ Output: final_answer                   │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Tool Definitions
//! The agent has access to these tools:
//! - **get_weather**: Get current weather for a location
//! - **calculate**: Perform mathematical calculations
//! - **search_knowledge**: Search internal knowledge base
//!
//! ## Prerequisites
//! 1. Conductor server with AI/LLM support (Orkes Conductor)
//! 2. LLM provider configured (e.g., OpenAI with function calling support)
//! 3. Task workers registered for each tool
//!
//! ## Environment Variables
//! ```bash
//! export CONDUCTOR_SERVER_URL="https://your-conductor-server/api"
//! export CONDUCTOR_AUTH_KEY="your-key"
//! export CONDUCTOR_AUTH_SECRET="your-secret"
//! ```
//!
//! ## Run
//! ```bash
//! cargo run --example agentic_workflow
//! ```

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    models::{
        ChatMessage, StartWorkflowRequest, TaskDef, WorkflowDef, WorkflowStatus, WorkflowTask,
        WorkflowTimeoutPolicy,
    },
};

// Configuration
const LLM_PROVIDER: &str = "openai";
const LLM_MODEL: &str = "gpt-4o-mini";

/// Tool definitions in OpenAI function calling format
fn get_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get the current weather for a specific location",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city and state/country, e.g., 'San Francisco, CA'"
                        },
                        "unit": {
                            "type": "string",
                            "enum": ["celsius", "fahrenheit"],
                            "description": "Temperature unit"
                        }
                    },
                    "required": ["location"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "calculate",
                "description": "Perform mathematical calculations",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": {
                            "type": "string",
                            "description": "Mathematical expression to evaluate, e.g., '2 + 2 * 3'"
                        }
                    },
                    "required": ["expression"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_knowledge",
                "description": "Search the internal knowledge base for information",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of results to return",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "no_tool_needed",
                "description": "Use this when the user's question can be answered directly without any tools",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "answer": {
                            "type": "string",
                            "description": "Direct answer to the user's question"
                        }
                    },
                    "required": ["answer"]
                }
            }
        }
    ])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Agentic Workflow Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // Initialize the client
    let config = Configuration::default();
    let client = ConductorClient::new(config)?;
    let metadata_client = client.metadata_client();
    let workflow_client = client.workflow_client();

    // ==========================================================================
    // Register Tool Workers (Task Definitions)
    // ==========================================================================
    println!("\nREGISTERING TOOL TASK DEFINITIONS");
    println!("{}", "=".repeat(80));
    println!();

    let tool_tasks = vec![
        TaskDef::new("get_weather")
            .with_description("Gets weather for a location. Worker should return temperature and conditions."),
        TaskDef::new("calculate")
            .with_description("Evaluates a mathematical expression. Worker should return the result."),
        TaskDef::new("search_knowledge")
            .with_description("Searches knowledge base. Worker should return matching documents."),
    ];

    for task in &tool_tasks {
        match metadata_client.register_task_def(task).await {
            Ok(_) => println!("  Registered task: {}", task.name),
            Err(e) => println!("  Task {} may already exist: {}", task.name, e),
        }
    }

    // ==========================================================================
    // Create the Agent Workflow
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CREATING AGENT WORKFLOW");
    println!("{}", "=".repeat(80));
    println!();

    let workflow_name = "rust_ai_agent";

    // Step 1: LLM reasoning - decide which tool to use
    let system_prompt = r#"You are a helpful AI assistant with access to tools.
Analyze the user's question and decide which tool to use.

Available tools:
- get_weather: Get current weather for a location
- calculate: Perform mathematical calculations
- search_knowledge: Search the internal knowledge base
- no_tool_needed: Answer directly without tools

You MUST call exactly one function. Choose the most appropriate tool based on the user's question."#;

    let reasoning_task =
        WorkflowTask::llm_chat_complete("agent_reasoning_ref", LLM_PROVIDER, LLM_MODEL)
            .with_messages(vec![
                ChatMessage::system(system_prompt),
                ChatMessage::user("${workflow.input.question}"),
            ])
            .with_input_param("tools", get_tool_definitions())
            .with_input_param("tool_choice", "required") // Force tool use
            .with_temperature(0.0) // Deterministic for tool selection
            .with_max_tokens(500);

    // Step 2: Parse the tool call response
    let parse_script = r#"
    (function(){
        var output = $.llm_output;
        var toolCall = null;
        
        // Handle different response formats
        if (output.tool_calls && output.tool_calls.length > 0) {
            toolCall = output.tool_calls[0];
        } else if (output.function_call) {
            toolCall = {
                function: output.function_call
            };
        }
        
        if (toolCall) {
            var funcName = toolCall.function.name;
            var args = typeof toolCall.function.arguments === 'string' 
                ? JSON.parse(toolCall.function.arguments) 
                : toolCall.function.arguments;
            
            return {
                tool_name: funcName,
                tool_args: args,
                has_tool_call: true
            };
        }
        
        // Fallback - direct answer
        return {
            tool_name: 'no_tool_needed',
            tool_args: { answer: output.content || output.result || 'I could not process your request.' },
            has_tool_call: false
        };
    })();
    "#;

    let parse_task = WorkflowTask::inline("parse_tool_call_ref", parse_script)
        .with_input_param("llm_output", "${agent_reasoning_ref.output}");

    // Step 3: Switch based on tool selection
    let weather_task = WorkflowTask::simple("get_weather", "get_weather_ref")
        .with_input_param("location", "${parse_tool_call_ref.output.result.tool_args.location}")
        .with_input_param("unit", "${parse_tool_call_ref.output.result.tool_args.unit}");

    let calculate_task = WorkflowTask::simple("calculate", "calculate_ref")
        .with_input_param("expression", "${parse_tool_call_ref.output.result.tool_args.expression}");

    let search_task = WorkflowTask::simple("search_knowledge", "search_knowledge_ref")
        .with_input_param("query", "${parse_tool_call_ref.output.result.tool_args.query}")
        .with_input_param("max_results", "${parse_tool_call_ref.output.result.tool_args.max_results}");

    let direct_answer_task = WorkflowTask::inline(
        "direct_answer_ref",
        "(function(){ return { result: $.answer }; })();",
    )
    .with_input_param("answer", "${parse_tool_call_ref.output.result.tool_args.answer}");

    let tool_switch = WorkflowTask::switch_value_param(
        "tool_router_ref",
        "${parse_tool_call_ref.output.result.tool_name}",
    )
    .with_switch_case("get_weather", vec![weather_task])
    .with_switch_case("calculate", vec![calculate_task])
    .with_switch_case("search_knowledge", vec![search_task])
    .with_switch_case("no_tool_needed", vec![direct_answer_task])
    .with_default_case(vec![WorkflowTask::inline(
        "unknown_tool_ref",
        "(function(){ return { error: 'Unknown tool: ' + $.tool_name }; })();",
    )
    .with_input_param("tool_name", "${parse_tool_call_ref.output.result.tool_name}")]);

    // Step 4: Format the final response
    let format_script = r#"
    (function(){
        var toolName = $.tool_name;
        var toolResult = $.tool_result;
        var question = $.question;
        
        // Build response context
        var context = 'Tool used: ' + toolName + '\n';
        context += 'Result: ' + JSON.stringify(toolResult, null, 2);
        
        return {
            tool_used: toolName,
            tool_output: toolResult,
            question: question,
            summary: context
        };
    })();
    "#;

    let format_task = WorkflowTask::inline("format_response_ref", format_script)
        .with_input_param("tool_name", "${parse_tool_call_ref.output.result.tool_name}")
        .with_input_param("tool_result", "${tool_router_ref.output}")
        .with_input_param("question", "${workflow.input.question}");

    // Build the agent workflow
    let workflow = WorkflowDef::new(workflow_name)
        .with_description("AI Agent: Analyzes questions and uses appropriate tools")
        .with_version(1)
        .with_task(reasoning_task)
        .with_task(parse_task)
        .with_task(tool_switch)
        .with_task(format_task)
        .with_input_parameters(vec!["question".to_string()])
        .with_output_param("tool_used", "${format_response_ref.output.result.tool_used}")
        .with_output_param("tool_output", "${format_response_ref.output.result.tool_output}")
        .with_output_param("summary", "${format_response_ref.output.result.summary}")
        .with_timeout(120, WorkflowTimeoutPolicy::TimeOutWf);

    println!("Workflow: {}", workflow.name);
    println!("Description: {:?}", workflow.description);
    println!();
    println!("Agent Pipeline:");
    println!("  1. agent_reasoning_ref  - LLM decides which tool to use");
    println!("  2. parse_tool_call_ref  - Parse tool call from LLM response");
    println!("  3. tool_router_ref      - Route to appropriate tool worker");
    println!("  4. format_response_ref  - Format final response");
    println!();

    // Register the workflow
    println!("Registering workflow...");
    metadata_client
        .register_or_update_workflow_def(&workflow, true)
        .await?;
    println!("  Workflow registered: {}", workflow_name);

    // ==========================================================================
    // Display Tool Definitions
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("TOOL DEFINITIONS");
    println!("{}", "=".repeat(80));
    println!();

    let tools = get_tool_definitions();
    if let Some(arr) = tools.as_array() {
        for tool in arr {
            if let Some(func) = tool.get("function") {
                let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let desc = func
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("?");
                println!("  {} - {}", name, desc);
            }
        }
    }

    // ==========================================================================
    // Example Queries
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("EXAMPLE QUERIES");
    println!("{}", "=".repeat(80));
    println!();

    let example_queries = vec![
        "What's the weather like in San Francisco?",
        "Calculate 15% of 250",
        "What is the capital of France?",
        "Search for information about Conductor workflows",
    ];

    for (i, query) in example_queries.iter().enumerate() {
        println!("  {}. {}", i + 1, query);
    }

    // ==========================================================================
    // Run Example (Weather Query)
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("RUNNING EXAMPLE QUERY");
    println!("{}", "=".repeat(80));
    println!();

    let test_question = "What's the weather like in Tokyo?";
    println!("Question: {}", test_question);
    println!();

    let request = StartWorkflowRequest::new(workflow_name)
        .with_version(1)
        .with_input_value("question", test_question);

    match workflow_client.start_workflow(&request).await {
        Ok(workflow_id) => {
            println!("Workflow started: {}", workflow_id);
            println!();

            // Poll for completion
            println!("Waiting for agent to process (max 30s)...");
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(30);

            loop {
                if start.elapsed() > timeout {
                    println!("  Timeout - the agent is still processing.");
                    println!("  This may happen if tool workers are not running.");
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                match workflow_client.get_workflow(&workflow_id, true).await {
                    Ok(wf) => {
                        let status = wf.status;
                        print!("\r  Status: {:?}          ", status);

                        if status == WorkflowStatus::Completed {
                            println!();
                            println!();
                            println!("{}", "-".repeat(60));
                            println!("AGENT RESPONSE");
                            println!("{}", "-".repeat(60));

                            if let Some(tool) = wf.output.get("tool_used") {
                                println!("Tool Selected: {}", tool);
                            }
                            if let Some(output) = wf.output.get("tool_output") {
                                println!(
                                    "Tool Output: {}",
                                    serde_json::to_string_pretty(output)?
                                );
                            }
                            if let Some(summary) = wf.output.get("summary") {
                                println!("\nSummary:\n{}", summary.as_str().unwrap_or("N/A"));
                            }
                            break;
                        } else if matches!(
                            status,
                            WorkflowStatus::Failed
                                | WorkflowStatus::Terminated
                                | WorkflowStatus::TimedOut
                        ) {
                            println!();
                            println!("  Agent workflow failed: {:?}", status);
                            if let Some(reason) = wf.reason_for_incompletion {
                                println!("  Reason: {}", reason);
                            }
                            println!();
                            println!("  Note: This is expected if:");
                            println!("    - Tool workers are not running");
                            println!("    - LLM integration is not configured");
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
            println!("  Could not start agent workflow: {}", e);
            println!();
            println!("  This is expected if LLM integration is not configured.");
        }
    }

    // ==========================================================================
    // How to Implement Tool Workers
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("IMPLEMENTING TOOL WORKERS");
    println!("{}", "=".repeat(80));
    println!();
    println!("To make the agent fully functional, implement workers for each tool:");
    println!();
    println!("```rust");
    println!("#[worker(get_weather)]");
    println!("async fn weather_worker(input: GetWeatherInput) -> TaskResult {{");
    println!("    // Call weather API");
    println!("    let weather = fetch_weather(&input.location).await?;");
    println!("    TaskResult::completed_with_output(json!({{");
    println!("        \"temperature\": weather.temp,");
    println!("        \"conditions\": weather.conditions");
    println!("    }}))");
    println!("}}");
    println!("```");
    println!();
    println!("See examples/worker_example.rs for worker implementation patterns.");

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

    // Note: We don't delete task definitions as they may be used by other workflows

    println!();
    println!("Agentic workflow example completed!");
    println!();
    println!("Key Concepts Demonstrated:");
    println!("  1. LLM with function/tool calling for decision making");
    println!("  2. Switch tasks for routing to different tool implementations");
    println!("  3. JavaScript inline tasks for parsing and formatting");
    println!("  4. Task workers as tool implementations");

    Ok(())
}
