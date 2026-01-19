# Prompt Management API Reference

Complete API reference for AI prompt template management in the Conductor Rust SDK.

> **Note**: Prompt management is available with Orkes Conductor.

## Table of Contents

- [Quick Start](#quick-start)
- [PromptClient API](#promptclient-api)
- [Managing Prompts](#managing-prompts)
- [Testing Prompts](#testing-prompts)
- [Tagging](#tagging)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("https://play.orkes.io/api")
        .with_auth("KEY_ID", "KEY_SECRET");
    
    let client = ConductorClient::new(config)?;
    let prompt_client = client.prompt_client();

    // Create a prompt template
    let template = r#"
You are a helpful assistant. Answer the following question:

Question: ${question}

Please provide a detailed response.
"#;

    prompt_client.save_prompt(
        "qa_assistant",              // name
        "Question answering prompt", // description
        template,                    // prompt template
        Some(&["gpt-4".to_string()]), // compatible models
        None,                        // version (auto)
        true,                        // auto increment version
    ).await?;

    println!("Prompt template created!");
    Ok(())
}
```

---

## PromptClient API

### Prompt Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `save_prompt()` | `POST /prompts/{name}` | Create or update a prompt |
| `get_prompt()` | `GET /prompts/{name}` | Get a prompt by name |
| `get_prompts()` | `GET /prompts` | Get all prompts |
| `delete_prompt()` | `DELETE /prompts/{name}` | Delete a prompt |
| `test_prompt()` | `POST /prompts/test` | Test a prompt with variables |

### Tag Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `get_tags_for_prompt_template()` | `GET /prompts/{name}/tags` | Get prompt tags |
| `update_tag_for_prompt_template()` | `PUT /prompts/{name}/tags` | Set prompt tags |
| `delete_tag_for_prompt_template()` | `DELETE /prompts/{name}/tags` | Delete prompt tags |

---

## Managing Prompts

### Create a Prompt Template

```rust
// Simple prompt
let template = "Hello, ${name}! Welcome to ${company}.";

prompt_client.save_prompt(
    "greeting_prompt",
    "A friendly greeting template",
    template,
    None,   // models
    None,   // version
    false,  // auto_increment
).await?;

// Prompt with model specifications
let ai_template = r#"
You are an expert ${role}. Given the following context:

Context: ${context}

Task: ${task}

Provide a detailed response following these guidelines:
1. Be accurate and professional
2. Include relevant examples
3. Structure your response clearly
"#;

prompt_client.save_prompt(
    "expert_assistant",
    "Expert assistant prompt for various domains",
    ai_template,
    Some(&["gpt-4".to_string(), "gpt-3.5-turbo".to_string()]),
    Some(1),  // explicit version
    false,
).await?;
```

### Update a Prompt Template

```rust
// Update with auto-increment version
let updated_template = r#"
You are an expert ${role} with extensive experience.

Context: ${context}
Task: ${task}
Additional Notes: ${notes}

Provide a comprehensive response.
"#;

prompt_client.save_prompt(
    "expert_assistant",
    "Updated expert assistant prompt",
    updated_template,
    Some(&["gpt-4".to_string()]),
    None,     // version will be auto-incremented
    true,     // auto_increment = true
).await?;
```

### Get a Prompt Template

```rust
let prompt = prompt_client.get_prompt("expert_assistant").await?;

println!("Name: {}", prompt.name);
println!("Description: {}", prompt.description);
println!("Template: {}", prompt.template);
println!("Variables: {:?}", prompt.variables);
if let Some(models) = &prompt.models {
    println!("Models: {:?}", models);
}
```

### List All Prompts

```rust
let prompts = prompt_client.get_prompts().await?;

for prompt in &prompts {
    println!("{}: {}", prompt.name, prompt.description);
}
```

### Delete a Prompt Template

```rust
prompt_client.delete_prompt("old_prompt").await?;
println!("Prompt deleted");
```

---

## Testing Prompts

### Test a Prompt with Variables

```rust
use std::collections::HashMap;

let prompt_text = r#"
Translate the following text from ${source_language} to ${target_language}:

Text: ${text}

Translation:
"#;

let mut variables = HashMap::new();
variables.insert("source_language".to_string(), json!("English"));
variables.insert("target_language".to_string(), json!("Spanish"));
variables.insert("text".to_string(), json!("Hello, how are you?"));

let result = prompt_client.test_prompt(
    prompt_text,
    &variables,
    "openai",           // AI integration provider
    "gpt-3.5-turbo",    // model
    0.7,                // temperature
    1.0,                // top_p
    None,               // stop_words
).await?;

println!("Response: {}", result);
```

### Test with Stop Words

```rust
let result = prompt_client.test_prompt(
    "List 5 programming languages:\n1.",
    &HashMap::new(),
    "openai",
    "gpt-3.5-turbo",
    0.7,
    1.0,
    Some(&["6.".to_string()]),  // Stop after 5 items
).await?;
```

---

## Tagging

### Set Tags

```rust
use conductor::models::MetadataTag;

let tags = vec![
    MetadataTag::new("category", "customer-service"),
    MetadataTag::new("version", "production"),
    MetadataTag::new("team", "support"),
];

prompt_client.update_tag_for_prompt_template("qa_assistant", &tags).await?;
```

### Get Tags

```rust
let tags = prompt_client.get_tags_for_prompt_template("qa_assistant").await?;

for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}
```

### Delete Tags

```rust
let tags_to_delete = vec![
    MetadataTag::new("version", "production"),
];

prompt_client.delete_tag_for_prompt_template("qa_assistant", &tags_to_delete).await?;
```

---

## Using Prompts in Workflows

### LLM Task with Prompt

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("ai_workflow")
    .with_task(
        WorkflowTask::llm_text("generate_response")
            .with_input_param("llmProvider", "openai")
            .with_input_param("model", "gpt-4")
            .with_input_param("promptName", "qa_assistant")
            .with_input_param("promptVariables", serde_json::json!({
                "question": "${workflow.input.question}"
            }))
    );
```

---

## Best Practices

### 1. Use Descriptive Variable Names

```rust
// Good - clear variable names
let template = r#"
Customer Name: ${customer_name}
Order ID: ${order_id}
Issue: ${customer_issue}

Please help resolve this customer's issue.
"#;

// Bad - ambiguous variables
let template = "Name: ${n}, ID: ${i}, Problem: ${p}";
```

### 2. Version Your Prompts

```rust
// Always use auto_increment for production prompts
prompt_client.save_prompt(
    "production_prompt",
    "Production prompt - v2 with improved instructions",
    template,
    Some(&["gpt-4".to_string()]),
    None,     // Let system manage version
    true,     // Enable auto-increment
).await?;
```

### 3. Test Before Deploying

```rust
async fn validate_prompt(
    client: &PromptClient,
    prompt_name: &str,
    test_variables: HashMap<String, serde_json::Value>,
) -> Result<bool> {
    // Get the prompt
    let prompt = client.get_prompt(prompt_name).await?;
    
    // Test with sample data
    let result = client.test_prompt(
        &prompt.template,
        &test_variables,
        "openai",
        "gpt-3.5-turbo",  // Use cheaper model for testing
        0.7,
        1.0,
        None,
    ).await?;
    
    // Validate response
    if result.is_empty() {
        println!("Warning: Empty response from prompt");
        return Ok(false);
    }
    
    println!("Test successful: {}", &result[..100.min(result.len())]);
    Ok(true)
}
```

### 4. Use Tags for Organization

```rust
// Tag prompts by use case and status
let production_tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("use_case", "customer_support"),
    MetadataTag::new("owner", "ai-team"),
    MetadataTag::new("reviewed", "true"),
];

prompt_client.update_tag_for_prompt_template("support_prompt", &production_tags).await?;
```

### 5. Document Prompt Variables

```rust
// Include documentation in the description
let description = r#"
Customer support prompt for handling inquiries.

Variables:
- customer_name: Customer's full name
- issue_category: Type of issue (billing, technical, general)
- issue_description: Detailed description of the issue
- priority: Priority level (low, medium, high)

Model recommendations: gpt-4 for complex issues, gpt-3.5-turbo for simple queries
"#;

prompt_client.save_prompt(
    "support_prompt",
    description,
    template,
    Some(&["gpt-4".to_string(), "gpt-3.5-turbo".to_string()]),
    None,
    true,
).await?;
```

---

## See Also

- [Integration Management](./INTEGRATION.md) - AI provider integrations
- [Workflow Management](./WORKFLOW.md) - Using prompts in workflows
