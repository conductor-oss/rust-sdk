# Secret Management API Reference

Complete API reference for secret management operations in the Conductor Rust SDK.

> **Note**: Secret management is available with Orkes Conductor. Secrets are encrypted at rest and in transit.

## Table of Contents

- [Quick Start](#quick-start)
- [SecretClient API](#secretclient-api)
- [Managing Secrets](#managing-secrets)
- [Tagging](#tagging)
- [Using Secrets in Workflows](#using-secrets-in-workflows)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::MetadataTag;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("https://play.orkes.io/api")
        .with_auth("KEY_ID", "KEY_SECRET");
    
    let client = ConductorClient::new(config)?;
    let secret_client = client.secret_client();

    // Store a secret
    secret_client.put_secret("API_KEY", "sk-1234567890abcdef").await?;

    // Retrieve a secret
    let api_key = secret_client.get_secret("API_KEY").await?;
    println!("API Key: {}...", &api_key[..10]);

    // List all secrets
    let secret_names = secret_client.list_all_secret_names().await?;
    println!("Available secrets: {:?}", secret_names);

    Ok(())
}
```

---

## SecretClient API

### Secret Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `put_secret()` | `PUT /secrets/{key}` | Store or update a secret |
| `get_secret()` | `GET /secrets/{key}` | Retrieve a secret value |
| `delete_secret()` | `DELETE /secrets/{key}` | Delete a secret |
| `secret_exists()` | `GET /secrets/{key}/exists` | Check if secret exists |
| `list_all_secret_names()` | `GET /secrets` | List all secret names |
| `list_secrets_that_user_can_grant_access_to()` | `GET /secrets?grantable=true` | List grantable secrets |

### Tag Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| `set_secret_tags()` | `PUT /secrets/{key}/tags` | Set secret tags |
| `get_secret_tags()` | `GET /secrets/{key}/tags` | Get secret tags |
| `delete_secret_tags()` | `DELETE /secrets/{key}/tags` | Delete secret tags |

---

## Managing Secrets

### Store a Secret

```rust
// Store a simple secret
secret_client.put_secret("DATABASE_PASSWORD", "super_secure_password").await?;

// Store API credentials
secret_client.put_secret("STRIPE_API_KEY", "sk_live_abc123xyz").await?;

// Store JSON configuration
let config = serde_json::json!({
    "host": "db.example.com",
    "port": 5432,
    "ssl": true
});
secret_client.put_secret("DB_CONFIG", &config.to_string()).await?;

// Update an existing secret (same API)
secret_client.put_secret("DATABASE_PASSWORD", "new_password_123").await?;
```

### Retrieve a Secret

```rust
// Get a secret value
let password = secret_client.get_secret("DATABASE_PASSWORD").await?;

// Parse JSON secret
let config_str = secret_client.get_secret("DB_CONFIG").await?;
let config: serde_json::Value = serde_json::from_str(&config_str)?;
println!("Database host: {}", config["host"]);
```

### Check if Secret Exists

```rust
if secret_client.secret_exists("API_KEY").await? {
    let key = secret_client.get_secret("API_KEY").await?;
    // Use the key
} else {
    println!("API_KEY not configured!");
}
```

### List Secrets

```rust
// List all secret names
let secret_names = secret_client.list_all_secret_names().await?;

for name in &secret_names {
    println!("Secret: {}", name);
}

// Filter by prefix
let api_keys: Vec<_> = secret_names.iter()
    .filter(|s| s.starts_with("API_"))
    .collect();
println!("API secrets: {:?}", api_keys);
```

### List Grantable Secrets

```rust
// List secrets you can grant access to others
let grantable = secret_client.list_secrets_that_user_can_grant_access_to().await?;

println!("Secrets you can share:");
for name in &grantable {
    println!("  {}", name);
}
```

### Delete a Secret

```rust
secret_client.delete_secret("OLD_API_KEY").await?;
println!("Secret deleted");
```

---

## Tagging

### Set Secret Tags

```rust
use conductor::models::MetadataTag;

let tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("service", "payment-gateway"),
    MetadataTag::new("rotation_date", "2024-06-01"),
];

secret_client.set_secret_tags(&tags, "STRIPE_API_KEY").await?;
```

### Get Secret Tags

```rust
let tags = secret_client.get_secret_tags("STRIPE_API_KEY").await?;

for tag in &tags {
    println!("{}: {}", tag.key, tag.value);
}
```

### Delete Secret Tags

```rust
let tags_to_remove = vec![
    MetadataTag::new("rotation_date", "2024-06-01"),
];

secret_client.delete_secret_tags(&tags_to_remove, "STRIPE_API_KEY").await?;
```

---

## Using Secrets in Workflows

### Reference Secrets in Task Input

Secrets can be referenced in workflow task inputs using the `${workflow.secrets.SECRET_NAME}` syntax:

```rust
use conductor::models::{WorkflowDef, WorkflowTask};

let workflow = WorkflowDef::new("payment_workflow")
    .with_task(
        WorkflowTask::http("call_stripe")
            .with_input_param("http_request", serde_json::json!({
                "uri": "https://api.stripe.com/v1/charges",
                "method": "POST",
                "headers": {
                    "Authorization": "Bearer ${workflow.secrets.STRIPE_API_KEY}"
                },
                "body": {
                    "amount": "${workflow.input.amount}",
                    "currency": "usd"
                }
            }))
    );
```

### Secrets in Environment Variables

```rust
// Store connection strings
secret_client.put_secret(
    "DATABASE_URL",
    "postgresql://user:pass@host:5432/db",
).await?;

// Reference in task
let task = WorkflowTask::simple("db_task", "db_ref")
    .with_input_param("database_url", "${workflow.secrets.DATABASE_URL}");
```

---

## Best Practices

### 1. Use Descriptive Names

```rust
// Good - clear purpose and scope
secret_client.put_secret("PROD_STRIPE_SECRET_KEY", "sk_live_...").await?;
secret_client.put_secret("STAGING_AWS_ACCESS_KEY", "AKIA...").await?;

// Bad - ambiguous
secret_client.put_secret("key1", "value").await?;
secret_client.put_secret("password", "value").await?;
```

### 2. Validate Required Secrets on Startup

```rust
async fn validate_secrets(client: &SecretClient) -> Result<()> {
    let required = vec![
        "DATABASE_PASSWORD",
        "STRIPE_API_KEY",
        "JWT_SECRET",
    ];
    
    let mut missing = Vec::new();
    
    for secret_name in required {
        if !client.secret_exists(secret_name).await? {
            missing.push(secret_name);
        }
    }
    
    if !missing.is_empty() {
        return Err(format!("Missing required secrets: {:?}", missing).into());
    }
    
    Ok(())
}
```

### 3. Use Tags for Organization

```rust
// Tag by environment
let prod_secrets = vec!["PROD_DB_PASSWORD", "PROD_API_KEY"];
let prod_tag = MetadataTag::new("environment", "production");

for secret in prod_secrets {
    secret_client.set_secret_tags(&[prod_tag.clone()], secret).await?;
}

// Tag by team
let payment_tag = MetadataTag::new("team", "payments");
secret_client.set_secret_tags(&[payment_tag], "STRIPE_API_KEY").await?;
```

### 4. Implement Secret Rotation

```rust
async fn rotate_secret(
    client: &SecretClient,
    key: &str,
    new_value: &str,
) -> Result<()> {
    // Store backup of old secret (optional)
    if client.secret_exists(key).await? {
        let old_value = client.get_secret(key).await?;
        let backup_key = format!("{}_backup_{}", key, chrono::Utc::now().timestamp());
        client.put_secret(&backup_key, &old_value).await?;
        
        // Tag the backup
        client.set_secret_tags(&[
            MetadataTag::new("type", "backup"),
            MetadataTag::new("original", key),
        ], &backup_key).await?;
    }
    
    // Update the secret
    client.put_secret(key, new_value).await?;
    
    // Update rotation tag
    client.set_secret_tags(&[
        MetadataTag::new("last_rotated", &chrono::Utc::now().to_rfc3339()),
    ], key).await?;
    
    Ok(())
}
```

### 5. Environment-Specific Secrets

```rust
struct EnvSecrets<'a> {
    client: &'a SecretClient,
    env: String,
}

impl<'a> EnvSecrets<'a> {
    fn new(client: &'a SecretClient, env: &str) -> Self {
        Self {
            client,
            env: env.to_uppercase(),
        }
    }
    
    async fn get(&self, key: &str) -> Result<String> {
        let full_key = format!("{}_{}", self.env, key);
        self.client.get_secret(&full_key).await
    }
    
    async fn put(&self, key: &str, value: &str) -> Result<()> {
        let full_key = format!("{}_{}", self.env, key);
        self.client.put_secret(&full_key, value).await?;
        
        // Auto-tag with environment
        self.client.set_secret_tags(&[
            MetadataTag::new("environment", &self.env.to_lowercase()),
        ], &full_key).await
    }
}

// Usage
let prod_secrets = EnvSecrets::new(&secret_client, "PROD");
let api_key = prod_secrets.get("API_KEY").await?;  // Gets PROD_API_KEY
```

### 6. Audit Secret Access

```rust
// List all secrets with their tags for audit
async fn audit_secrets(client: &SecretClient) -> Result<()> {
    let names = client.list_all_secret_names().await?;
    
    println!("Secret Audit Report");
    println!("===================");
    
    for name in names {
        let tags = client.get_secret_tags(&name).await.unwrap_or_default();
        
        println!("\nSecret: {}", name);
        for tag in &tags {
            println!("  {}: {}", tag.key, tag.value);
        }
        
        // Check for missing required tags
        let has_env = tags.iter().any(|t| t.key == "environment");
        let has_team = tags.iter().any(|t| t.key == "team");
        
        if !has_env {
            println!("  WARNING: Missing 'environment' tag");
        }
        if !has_team {
            println!("  WARNING: Missing 'team' tag");
        }
    }
    
    Ok(())
}
```

---

## Error Handling

```rust
use conductor::error::ConductorError;

match secret_client.get_secret("MY_SECRET").await {
    Ok(value) => {
        println!("Secret value: {}", value);
    }
    Err(ConductorError::Server { status: 404, .. }) => {
        println!("Secret not found");
    }
    Err(ConductorError::Server { status: 403, .. }) => {
        println!("Access denied - check permissions");
    }
    Err(e) => {
        println!("Error: {}", e);
    }
}
```

---

## See Also

- [Authorization](./AUTHORIZATION.md) - Managing secret permissions
- [Workflow Management](./WORKFLOW.md) - Using secrets in workflows
