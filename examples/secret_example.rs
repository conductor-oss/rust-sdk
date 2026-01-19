//! Secret Management Example
//!
//! Demonstrates secret storage and retrieval.
//!
//! What it shows:
//! - Storing secrets
//! - Retrieving secrets
//! - Listing secrets
//! - Secret tags
//! - Checking secret existence
//!
//! Run with: cargo run --example secret_example
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080
//! - Set CONDUCTOR_SERVER_URL if using a different address

use conductor::{
    client::ConductorClient, configuration::Configuration, error::Result, models::MetadataTag,
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("conductor=info".parse().unwrap()),
        )
        .init();

    // Load configuration
    let config = Configuration::default();
    info!("Connecting to Conductor at {}", config.server_api_url);

    // Create the Conductor client
    let client = ConductorClient::new(config)?;
    let secret_client = client.secret_client();

    // ==============================
    // Store Secrets
    // ==============================
    info!("\n=== Storing Secrets ===");

    let secret_key = format!(
        "rust_demo_secret_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let secret_value = "my-secret-value-12345";

    info!("Storing secret: {}", secret_key);
    secret_client.put_secret(&secret_key, secret_value).await?;
    info!("Secret stored successfully");

    // Store another secret
    let api_key = format!(
        "rust_demo_api_key_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    secret_client
        .put_secret(&api_key, "api-key-value-abc123")
        .await?;
    info!("API key stored: {}", api_key);

    // ==============================
    // Check Secret Existence
    // ==============================
    info!("\n=== Checking Secret Existence ===");

    let exists = secret_client.secret_exists(&secret_key).await?;
    info!("Secret '{}' exists: {}", secret_key, exists);

    let nonexistent = secret_client.secret_exists("nonexistent_secret").await?;
    info!("Nonexistent secret exists: {}", nonexistent);

    // ==============================
    // Retrieve Secrets
    // ==============================
    info!("\n=== Retrieving Secrets ===");

    let retrieved = secret_client.get_secret(&secret_key).await?;
    info!("Retrieved secret value: {}", retrieved);

    // ==============================
    // List Secrets
    // ==============================
    info!("\n=== Listing Secrets ===");

    let all_secrets = secret_client.list_all_secret_names().await?;
    info!("Total secrets: {}", all_secrets.len());

    // Show first few
    for (i, name) in all_secrets.iter().take(5).enumerate() {
        info!("  {}: {}", i + 1, name);
    }
    if all_secrets.len() > 5 {
        info!("  ... and {} more", all_secrets.len() - 5);
    }

    // List secrets user can grant access to
    let grantable = secret_client
        .list_secrets_that_user_can_grant_access_to()
        .await?;
    info!("Secrets user can grant access to: {}", grantable.len());

    // ==============================
    // Secret Tags
    // ==============================
    info!("\n=== Managing Secret Tags ===");

    let tags = vec![
        MetadataTag::with_value("environment", "demo"),
        MetadataTag::with_value("team", "platform"),
    ];

    info!("Setting tags on secret...");
    secret_client.set_secret_tags(&tags, &secret_key).await?;

    let retrieved_tags = secret_client.get_secret_tags(&secret_key).await?;
    info!("Retrieved tags:");
    for tag in &retrieved_tags {
        info!("  {} = {:?}", tag.key, tag.value);
    }

    // Delete one tag
    info!("Deleting a tag...");
    secret_client
        .delete_secret_tags(&[tags[0].clone()], &secret_key)
        .await?;

    let remaining_tags = secret_client.get_secret_tags(&secret_key).await?;
    info!("Remaining tags: {}", remaining_tags.len());

    // ==============================
    // Update Secret
    // ==============================
    info!("\n=== Updating Secret ===");

    let new_value = "updated-secret-value-67890";
    secret_client.put_secret(&secret_key, new_value).await?;
    info!("Secret updated");

    let updated = secret_client.get_secret(&secret_key).await?;
    info!("New secret value: {}", updated);

    // ==============================
    // Cleanup
    // ==============================
    info!("\n=== Cleanup ===");

    secret_client.delete_secret(&secret_key).await?;
    info!("Deleted secret: {}", secret_key);

    secret_client.delete_secret(&api_key).await?;
    info!("Deleted API key: {}", api_key);

    // Verify deletion
    let still_exists = secret_client.secret_exists(&secret_key).await?;
    info!("Secret still exists after deletion: {}", still_exists);

    info!("\nSecret management example completed!");
    Ok(())
}
