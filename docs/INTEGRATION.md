# Integration Management API Reference

Complete API reference for managing external system integrations in the Conductor Rust SDK.

> **Note**: Integration management is available with Orkes Conductor.

## Table of Contents

- [Quick Start](#quick-start)
- [IntegrationClient API](#integrationclient-api)
- [Managing Integrations](#managing-integrations)
- [Managing Integration APIs](#managing-integration-apis)
- [Prompt Associations](#prompt-associations)
- [Token Usage Metrics](#token-usage-metrics)
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
    let integration_client = client.integration_client();

    // Create an OpenAI integration
    let mut config = std::collections::HashMap::new();
    config.insert("api_key".to_string(), serde_json::json!("sk-..."));

    let integration = IntegrationUpdate {
        integration_type: Some("openai".to_string()),
        category: Some("AI_MODEL".to_string()),
        enabled: Some(true),
        description: Some("OpenAI GPT integration".to_string()),
        configuration: config,
    };

    integration_client.save_integration("my_openai", &integration).await?;

    println!("Integration created!");
    Ok(())
}
```

---

## IntegrationClient API

### Integration Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `save_integration()` | `PUT /integrations/provider/{name}` | Create or update an integration |
| `get_integration()` | `GET /integrations/provider/{name}` | Get an integration by name |
| `get_integrations()` | `GET /integrations/provider` | Get all integrations |
| `delete_integration()` | `DELETE /integrations/provider/{name}` | Delete an integration |
| `get_integration_provider_defs()` | `GET /integrations/def` | Get all provider definitions |
| `get_providers_and_integrations()` | `GET /integrations` | Get all providers and their integrations |

### Integration API Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `save_integration_api()` | `PUT /integrations/provider/{name}/integration/{api}` | Create or update an integration API |
| `get_integration_api()` | `GET /integrations/provider/{name}/integration/{api}` | Get an integration API |
| `get_integration_apis()` | `GET /integrations/provider/{name}/integration` | Get all APIs for an integration |
| `delete_integration_api()` | `DELETE /integrations/provider/{name}/integration/{api}` | Delete an integration API |
| `get_integration_available_apis()` | `GET /integrations/provider/{name}/models` | Get available APIs for an integration |

### Prompt Association

| Method | Endpoint | Description |
|--------|----------|-------------|
| `associate_prompt_with_integration()` | `POST /integrations/provider/{name}/integration/{model}/prompt/{prompt}` | Associate a prompt with a model |
| `get_prompts_with_integration()` | `GET /integrations/provider/{name}/integration/{model}/prompt` | Get prompts for a model |

### Token Usage Metrics

| Method | Endpoint | Description |
|--------|----------|-------------|
| `get_token_usage_for_integration()` | `GET /integrations/provider/{name}/integration/{api}/metrics` | Get token usage for an API |
| `get_token_usage_for_integration_provider()` | `GET /integrations/provider/{name}/metrics` | Get token usage for a provider |

### Tag Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `put_tag_for_integration()` | `PUT /integrations/provider/{name}/integration/{api}/tags` | Set tags for an integration API |
| `get_tags_for_integration()` | `GET /integrations/provider/{name}/integration/{api}/tags` | Get tags for an integration API |
| `delete_tag_for_integration()` | `DELETE /integrations/provider/{name}/integration/{api}/tags` | Delete tags from an integration API |
| `put_tag_for_integration_provider()` | `PUT /integrations/provider/{name}/tags` | Set tags for a provider |
| `get_tags_for_integration_provider()` | `GET /integrations/provider/{name}/tags` | Get tags for a provider |
| `delete_tag_for_integration_provider()` | `DELETE /integrations/provider/{name}/tags` | Delete tags from a provider |

---

## Managing Integrations

### Create an Integration

```rust
use conductor::models::IntegrationUpdate;
use std::collections::HashMap;

// Create an OpenAI integration
let mut config = HashMap::new();
config.insert("api_key".to_string(), serde_json::json!("sk-your-api-key"));

let integration = IntegrationUpdate {
    integration_type: Some("openai".to_string()),
    category: Some("AI_MODEL".to_string()),
    enabled: Some(true),
    description: Some("Production OpenAI integration".to_string()),
    configuration: config,
};

integration_client.save_integration("openai_prod", &integration).await?;
```

### Create Different Integration Types

```rust
// Vector Database (Pinecone)
let mut pinecone_config = HashMap::new();
pinecone_config.insert("api_key".to_string(), serde_json::json!("pc-your-key"));
pinecone_config.insert("environment".to_string(), serde_json::json!("us-east-1"));

let pinecone = IntegrationUpdate {
    integration_type: Some("pinecone".to_string()),
    category: Some("VECTOR_DB".to_string()),
    enabled: Some(true),
    description: Some("Pinecone vector database".to_string()),
    configuration: pinecone_config,
};

integration_client.save_integration("pinecone_prod", &pinecone).await?;

// Message Queue (Kafka)
let mut kafka_config = HashMap::new();
kafka_config.insert("bootstrap.servers".to_string(), serde_json::json!("kafka:9092"));

let kafka = IntegrationUpdate {
    integration_type: Some("kafka".to_string()),
    category: Some("MESSAGE_BROKER".to_string()),
    enabled: Some(true),
    description: Some("Kafka message broker".to_string()),
    configuration: kafka_config,
};

integration_client.save_integration("kafka_prod", &kafka).await?;

// Anthropic Claude
let mut claude_config = HashMap::new();
claude_config.insert("api_key".to_string(), serde_json::json!("sk-ant-your-key"));

let claude = IntegrationUpdate {
    integration_type: Some("anthropic".to_string()),
    category: Some("AI_MODEL".to_string()),
    enabled: Some(true),
    description: Some("Anthropic Claude integration".to_string()),
    configuration: claude_config,
};

integration_client.save_integration("claude_prod", &claude).await?;
```

### Get an Integration

```rust
let integration = integration_client.get_integration("openai_prod").await?;

println!("Name: {}", integration.name);
println!("Type: {:?}", integration.integration_type);
println!("Category: {:?}", integration.category);
println!("Enabled: {}", integration.enabled);
if let Some(desc) = &integration.description {
    println!("Description: {}", desc);
}
```

### List All Integrations

```rust
let integrations = integration_client.get_integrations().await?;

for integration in &integrations {
    println!("{} ({:?}) - Enabled: {}", 
        integration.name, 
        integration.integration_type,
        integration.enabled
    );
}
```

### Get Provider Definitions

```rust
// Get all available integration provider definitions
let provider_defs = integration_client.get_integration_provider_defs().await?;

for def in &provider_defs {
    println!("Provider: {}", serde_json::to_string_pretty(def)?);
}
```

### Get All Providers and Integrations

```rust
// Get a complete view of all providers and their integrations
let all_providers = integration_client.get_providers_and_integrations().await?;
println!("{}", serde_json::to_string_pretty(&all_providers)?);
```

### Update an Integration

```rust
// Update configuration
let mut updated_config = HashMap::new();
updated_config.insert("api_key".to_string(), serde_json::json!("sk-new-api-key"));
updated_config.insert("organization".to_string(), serde_json::json!("org-123"));

let update = IntegrationUpdate {
    integration_type: None,  // Keep existing
    category: None,          // Keep existing
    enabled: Some(true),
    description: Some("Updated OpenAI integration with org".to_string()),
    configuration: updated_config,
};

integration_client.save_integration("openai_prod", &update).await?;
```

### Delete an Integration

```rust
integration_client.delete_integration("old_integration").await?;
println!("Integration deleted");
```

---

## Managing Integration APIs

Integration APIs represent specific models, indexes, or endpoints within an integration (e.g., GPT-4 within OpenAI, or a specific index in Pinecone).

### Create an Integration API

```rust
use conductor::models::IntegrationApiUpdate;

// Add GPT-4 model to OpenAI integration
let mut model_config = HashMap::new();
model_config.insert("max_tokens".to_string(), serde_json::json!(4096));
model_config.insert("temperature".to_string(), serde_json::json!(0.7));

let gpt4_api = IntegrationApiUpdate {
    configuration: model_config,
    enabled: Some(true),
    description: Some("GPT-4 model for complex reasoning".to_string()),
};

integration_client.save_integration_api(
    "openai_prod",  // integration name
    "gpt-4",        // API/model name
    &gpt4_api,
).await?;

// Add GPT-3.5-turbo for simpler tasks
let mut turbo_config = HashMap::new();
turbo_config.insert("max_tokens".to_string(), serde_json::json!(2048));

let turbo_api = IntegrationApiUpdate {
    configuration: turbo_config,
    enabled: Some(true),
    description: Some("GPT-3.5 Turbo for cost-effective operations".to_string()),
};

integration_client.save_integration_api(
    "openai_prod",
    "gpt-3.5-turbo",
    &turbo_api,
).await?;
```

### Get an Integration API

```rust
let api = integration_client.get_integration_api(
    "gpt-4",        // API name
    "openai_prod",  // integration name
).await?;

println!("API: {}", api.name);
println!("Integration: {}", api.integration_name);
println!("Enabled: {}", api.enabled);
println!("Config: {:?}", api.configuration);
```

### List All APIs for an Integration

```rust
let apis = integration_client.get_integration_apis("openai_prod").await?;

for api in &apis {
    println!("{}: {} (enabled: {})", 
        api.name, 
        api.description.as_deref().unwrap_or("No description"),
        api.enabled
    );
}
```

### Get Available APIs for an Integration

```rust
// Discover what models/APIs are available for an integration provider
let available = integration_client.get_integration_available_apis("openai_prod").await?;

println!("Available models:");
for model in &available {
    println!("  - {}", model);
}
```

### Delete an Integration API

```rust
integration_client.delete_integration_api(
    "gpt-3.5-turbo",  // API name
    "openai_prod",    // integration name
).await?;
```

---

## Prompt Associations

Associate prompt templates with specific AI models for use in LLM tasks.

### Associate a Prompt with a Model

```rust
// First, create a prompt template (see PROMPT.md)
// Then associate it with an AI model

integration_client.associate_prompt_with_integration(
    "openai_prod",      // AI integration name
    "gpt-4",            // model name
    "customer_support", // prompt name
).await?;

println!("Prompt associated with model");
```

### Get Prompts for a Model

```rust
let prompts = integration_client.get_prompts_with_integration(
    "openai_prod",  // AI integration name
    "gpt-4",        // model name
).await?;

println!("Prompts for GPT-4:");
for prompt in &prompts {
    println!("  - {}: {}", prompt.name, prompt.description);
}
```

### Associate Multiple Prompts

```rust
// Associate the same prompt with multiple models
let prompt_name = "general_assistant";
let models = ["gpt-4", "gpt-3.5-turbo", "gpt-4-turbo"];

for model in &models {
    integration_client.associate_prompt_with_integration(
        "openai_prod",
        model,
        prompt_name,
    ).await?;
    println!("Associated {} with {}", prompt_name, model);
}
```

---

## Token Usage Metrics

Monitor token usage for cost tracking and optimization.

### Get Token Usage for a Model

```rust
let token_count = integration_client.get_token_usage_for_integration(
    "gpt-4",        // API/model name
    "openai_prod",  // integration name
).await?;

println!("Token usage for GPT-4: {}", token_count);
```

### Get Token Usage for a Provider

```rust
let usage = integration_client.get_token_usage_for_integration_provider(
    "openai_prod",  // integration name
).await?;

println!("Provider usage: {}", serde_json::to_string_pretty(&usage)?);
```

### Monitor All Model Usage

```rust
async fn print_usage_report(client: &IntegrationClient, integration_name: &str) -> Result<()> {
    let apis = client.get_integration_apis(integration_name).await?;
    
    println!("Token Usage Report for {}:", integration_name);
    println!("{:-<50}", "");
    
    let mut total = 0i64;
    for api in &apis {
        let usage = client.get_token_usage_for_integration(
            &api.name,
            integration_name,
        ).await?;
        
        println!("  {:<20} {:>15} tokens", api.name, usage);
        total += usage;
    }
    
    println!("{:-<50}", "");
    println!("  {:<20} {:>15} tokens", "TOTAL", total);
    
    Ok(())
}

// Usage
print_usage_report(&integration_client, "openai_prod").await?;
```

---

## Tagging

### Tag an Integration Provider

```rust
use conductor::models::MetadataTag;

let tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("team", "ai-platform"),
    MetadataTag::new("cost_center", "engineering"),
];

integration_client.put_tag_for_integration_provider(&tags, "openai_prod").await?;
```

### Get Provider Tags

```rust
let tags = integration_client.get_tags_for_integration_provider("openai_prod").await?;

for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}
```

### Delete Provider Tags

```rust
let tags_to_delete = vec![
    MetadataTag::new("cost_center", "engineering"),
];

integration_client.delete_tag_for_integration_provider(&tags_to_delete, "openai_prod").await?;
```

### Tag an Integration API

```rust
let model_tags = vec![
    MetadataTag::new("use_case", "customer_support"),
    MetadataTag::new("tier", "premium"),
    MetadataTag::new("rate_limit", "10000"),
];

integration_client.put_tag_for_integration(
    &model_tags,
    "openai_prod",  // integration name
    "gpt-4",        // API name
).await?;
```

### Get API Tags

```rust
let tags = integration_client.get_tags_for_integration(
    "openai_prod",
    "gpt-4",
).await?;

for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}
```

### Delete API Tags

```rust
let tags_to_delete = vec![
    MetadataTag::new("tier", "premium"),
];

integration_client.delete_tag_for_integration(
    &tags_to_delete,
    "openai_prod",
    "gpt-4",
).await?;
```

---

## Best Practices

### 1. Use Environment-Specific Integrations

```rust
// Create separate integrations for different environments
async fn setup_environment_integrations(
    client: &IntegrationClient,
    env: &str,
    api_key: &str,
) -> Result<()> {
    let mut config = HashMap::new();
    config.insert("api_key".to_string(), serde_json::json!(api_key));
    
    let integration = IntegrationUpdate {
        integration_type: Some("openai".to_string()),
        category: Some("AI_MODEL".to_string()),
        enabled: Some(true),
        description: Some(format!("OpenAI integration for {} environment", env)),
        configuration: config,
    };
    
    let name = format!("openai_{}", env);
    client.save_integration(&name, &integration).await?;
    
    // Tag with environment
    let tags = vec![MetadataTag::new("environment", env)];
    client.put_tag_for_integration_provider(&tags, &name).await?;
    
    Ok(())
}

// Usage
setup_environment_integrations(&client, "dev", "sk-dev-key").await?;
setup_environment_integrations(&client, "staging", "sk-staging-key").await?;
setup_environment_integrations(&client, "prod", "sk-prod-key").await?;
```

### 2. Implement Cost Monitoring

```rust
async fn check_usage_limits(
    client: &IntegrationClient,
    integration_name: &str,
    daily_limit: i64,
) -> Result<bool> {
    let usage = client.get_token_usage_for_integration_provider(integration_name).await?;
    
    // Parse usage (structure depends on provider)
    let current_usage = usage.get("daily_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    if current_usage > daily_limit {
        println!("WARNING: Daily limit exceeded for {}", integration_name);
        println!("Current: {}, Limit: {}", current_usage, daily_limit);
        return Ok(false);
    }
    
    let percentage = (current_usage as f64 / daily_limit as f64) * 100.0;
    if percentage > 80.0 {
        println!("ALERT: {} is at {:.1}% of daily limit", integration_name, percentage);
    }
    
    Ok(true)
}
```

### 3. Centralized Integration Setup

```rust
use conductor::models::{IntegrationUpdate, IntegrationApiUpdate};

struct IntegrationSetup {
    client: IntegrationClient,
}

impl IntegrationSetup {
    pub async fn setup_openai(
        &self,
        name: &str,
        api_key: &str,
        models: &[&str],
    ) -> Result<()> {
        // Create integration
        let mut config = HashMap::new();
        config.insert("api_key".to_string(), serde_json::json!(api_key));
        
        let integration = IntegrationUpdate {
            integration_type: Some("openai".to_string()),
            category: Some("AI_MODEL".to_string()),
            enabled: Some(true),
            description: Some("OpenAI integration".to_string()),
            configuration: config,
        };
        
        self.client.save_integration(name, &integration).await?;
        
        // Setup models
        for model in models {
            let api = IntegrationApiUpdate {
                configuration: HashMap::new(),
                enabled: Some(true),
                description: Some(format!("{} model", model)),
            };
            
            self.client.save_integration_api(name, model, &api).await?;
        }
        
        Ok(())
    }
    
    pub async fn setup_vector_db(
        &self,
        name: &str,
        provider: &str,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let integration = IntegrationUpdate {
            integration_type: Some(provider.to_string()),
            category: Some("VECTOR_DB".to_string()),
            enabled: Some(true),
            description: Some(format!("{} vector database", provider)),
            configuration: config,
        };
        
        self.client.save_integration(name, &integration).await
    }
}
```

### 4. Disable Instead of Delete

```rust
// Prefer disabling integrations over deleting them
async fn disable_integration(
    client: &IntegrationClient,
    name: &str,
) -> Result<()> {
    let update = IntegrationUpdate {
        integration_type: None,
        category: None,
        enabled: Some(false),  // Disable, don't delete
        description: None,
        configuration: HashMap::new(),
    };
    
    client.save_integration(name, &update).await?;
    
    // Tag as disabled with timestamp
    let tags = vec![
        MetadataTag::new("status", "disabled"),
        MetadataTag::new("disabled_at", &chrono::Utc::now().to_rfc3339()),
    ];
    client.put_tag_for_integration_provider(&tags, name).await?;
    
    Ok(())
}
```

### 5. Validate Integration Health

```rust
async fn validate_integration_setup(
    client: &IntegrationClient,
    integration_name: &str,
) -> Result<bool> {
    // Check integration exists and is enabled
    let integration = match client.get_integration(integration_name).await {
        Ok(i) => i,
        Err(e) => {
            println!("Integration not found: {}", e);
            return Ok(false);
        }
    };
    
    if !integration.enabled {
        println!("Integration is disabled");
        return Ok(false);
    }
    
    // Check for configured APIs
    let apis = client.get_integration_apis(integration_name).await?;
    let enabled_apis: Vec<_> = apis.iter().filter(|a| a.enabled).collect();
    
    if enabled_apis.is_empty() {
        println!("No enabled APIs found for integration");
        return Ok(false);
    }
    
    println!("Integration {} is healthy with {} enabled APIs", 
        integration_name, 
        enabled_apis.len()
    );
    
    Ok(true)
}
```

---

## Models Reference

### Integration

```rust
pub struct Integration {
    pub name: String,
    pub integration_type: Option<String>,  // e.g., "openai", "pinecone", "kafka"
    pub category: Option<String>,          // e.g., "AI_MODEL", "VECTOR_DB", "MESSAGE_BROKER"
    pub enabled: bool,
    pub description: Option<String>,
    pub configuration: HashMap<String, serde_json::Value>,
    pub create_time: Option<i64>,
    pub created_by: Option<String>,
    pub update_time: Option<i64>,
    pub updated_by: Option<String>,
}
```

### IntegrationUpdate

```rust
pub struct IntegrationUpdate {
    pub integration_type: Option<String>,
    pub category: Option<String>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
    pub configuration: HashMap<String, serde_json::Value>,
}
```

### IntegrationApi

```rust
pub struct IntegrationApi {
    pub name: String,
    pub integration_name: String,
    pub configuration: HashMap<String, serde_json::Value>,
    pub enabled: bool,
    pub description: Option<String>,
    pub create_time: Option<i64>,
    pub created_by: Option<String>,
    pub update_time: Option<i64>,
    pub updated_by: Option<String>,
}
```

### IntegrationApiUpdate

```rust
pub struct IntegrationApiUpdate {
    pub configuration: HashMap<String, serde_json::Value>,
    pub enabled: Option<bool>,
    pub description: Option<String>,
}
```

---

## See Also

- [Prompt Management](./PROMPT.md) - Managing AI prompts
- [Secret Management](./SECRET_MANAGEMENT.md) - Storing API keys securely
- [Authorization](./AUTHORIZATION.md) - Access control for integrations
