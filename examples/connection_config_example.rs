// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::time::Duration;

use conductor::{client::ConductorClient, configuration::Configuration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Connection Configuration Example - Conductor Rust SDK\n");
    println!("{}", "=".repeat(80));

    // ==========================================================================
    // Option 1: Default Configuration (from environment)
    // ==========================================================================
    println!("\n1. DEFAULT CONFIGURATION (from environment)");
    println!("{}", "-".repeat(60));
    println!();

    println!("Uses these environment variables:");
    println!("  CONDUCTOR_SERVER_URL   - Server API URL (default: http://localhost:8080/api)");
    println!("  CONDUCTOR_AUTH_KEY     - Authentication key (optional)");
    println!("  CONDUCTOR_AUTH_SECRET  - Authentication secret (optional)");
    println!("  CONDUCTOR_DEBUG        - Enable debug logging (true/false)");
    println!("  CONDUCTOR_TIMEOUT_SECS - Request timeout in seconds (default: 30)");
    println!();

    println!("Code:");
    println!("  let config = Configuration::default();");
    println!("  let client = ConductorClient::new(config)?;");
    println!();

    let default_config = Configuration::default();
    println!("Current settings:");
    println!("  Server URL: {}", default_config.server_api_url);
    println!(
        "  Auth configured: {}",
        default_config.auth_key.is_some() && default_config.auth_secret.is_some()
    );
    println!("  Timeout: {:?}", default_config.timeout);

    // ==========================================================================
    // Option 2: Programmatic Configuration
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("2. PROGRAMMATIC CONFIGURATION");
    println!("{}", "-".repeat(60));
    println!();

    println!("Code:");
    println!(r#"  let config = Configuration::new("https://conductor.example.com/api")"#);
    println!(r#"      .with_auth("your-key", "your-secret")"#);
    println!(r#"      .with_timeout(Duration::from_secs(60))"#);
    println!(r#"      .with_debug(true);"#);
    println!();

    let custom_config = Configuration::new("https://conductor.example.com/api")
        .with_auth("demo-key", "demo-secret")
        .with_timeout(Duration::from_secs(60))
        .with_debug(false);

    println!("Custom config:");
    println!("  Server URL: {}", custom_config.server_api_url);
    println!("  UI Host: {}", custom_config.ui_host);
    println!("  Auth configured: {}", custom_config.has_auth());
    println!("  Timeout: {:?}", custom_config.timeout);

    // ==========================================================================
    // Option 3: OSS Conductor (No Authentication)
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("3. OSS CONDUCTOR (No Authentication)");
    println!("{}", "-".repeat(60));
    println!();

    println!("For open-source Conductor without authentication:");
    println!();
    println!("Code:");
    println!(r#"  let config = Configuration::new("http://localhost:8080/api");"#);
    println!(r#"  // Simply don't set auth credentials"#);
    println!(r#"  let client = ConductorClient::new(config)?;"#);
    println!();

    let oss_config = Configuration::new("http://localhost:8080/api");
    println!("OSS config:");
    println!("  Server URL: {}", oss_config.server_api_url);
    println!("  Auth configured: {} (none needed for OSS)", oss_config.has_auth());

    // ==========================================================================
    // Option 4: Token TTL Configuration
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("4. TOKEN REFRESH CONFIGURATION");
    println!("{}", "-".repeat(60));
    println!();

    println!("The SDK automatically manages authentication tokens.");
    println!("Default TTL is 45 minutes. Customize if needed:");
    println!();
    println!("Code:");
    println!(r#"  let config = Configuration::new("https://conductor.example.com/api")"#);
    println!(r#"      .with_auth("key", "secret")"#);
    println!(r#"      .with_token_ttl(Duration::from_secs(30 * 60)); // 30 minutes"#);
    println!();

    let token_config = Configuration::new("https://example.com/api")
        .with_auth("key", "secret")
        .with_token_ttl(Duration::from_secs(30 * 60));

    println!("Token config:");
    println!("  Token TTL: {:?}", token_config.auth_token_ttl);
    println!(
        "  Token needs refresh: {}",
        token_config.token_needs_refresh()
    );

    // ==========================================================================
    // Option 5: Using .env Files
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("5. USING .env FILES");
    println!("{}", "-".repeat(60));
    println!();

    println!("Create a .env file in your project root:");
    println!();
    println!("```");
    println!("# .env file");
    println!("CONDUCTOR_SERVER_URL=https://your-server.example.com/api");
    println!("CONDUCTOR_AUTH_KEY=your-key-id");
    println!("CONDUCTOR_AUTH_SECRET=your-secret");
    println!("CONDUCTOR_TIMEOUT_SECS=60");
    println!("CONDUCTOR_DEBUG=false");
    println!("```");
    println!();
    println!("The SDK automatically loads .env files using the `dotenvy` crate.");

    // ==========================================================================
    // Testing Connectivity
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("6. TESTING CONNECTIVITY");
    println!("{}", "-".repeat(60));
    println!();

    println!("Test connectivity to your Conductor server:");
    println!();

    let config = Configuration::default();
    match ConductorClient::new(config) {
        Ok(client) => {
            println!("Client created successfully.");
            println!();

            // Try to list workflows as a connectivity test
            println!("Testing API connectivity...");
            let metadata_client = client.metadata_client();

            match metadata_client.get_all_workflow_defs().await {
                Ok(workflows) => {
                    println!("  Connection successful!");
                    println!("  Found {} workflow definitions", workflows.len());
                }
                Err(e) => {
                    println!("  Could not connect: {}", e);
                    println!();
                    println!("  Troubleshooting:");
                    println!("  - Check CONDUCTOR_SERVER_URL is correct");
                    println!("  - Verify the server is running");
                    println!("  - For Orkes, ensure auth credentials are valid");
                }
            }
        }
        Err(e) => {
            println!("  Could not create client: {}", e);
        }
    }

    // ==========================================================================
    // Working with Self-Signed Certificates
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("7. SELF-SIGNED CERTIFICATES (Development Only)");
    println!("{}", "-".repeat(60));
    println!();

    println!("If your development server uses self-signed certificates,");
    println!("you have these options:");
    println!();
    println!("Option A: Add the CA certificate to your system trust store");
    println!("  macOS:   security add-trusted-cert -d -r trustRoot -k ~/Library/Keychains/login.keychain cert.pem");
    println!("  Linux:   sudo cp cert.pem /usr/local/share/ca-certificates/ && sudo update-ca-certificates");
    println!("  Windows: Import via Certificate Manager");
    println!();
    println!("Option B: Set RUSTLS_NATIVE_CERTS_DIR or SSL_CERT_FILE");
    println!("  export SSL_CERT_FILE=/path/to/custom-ca-bundle.crt");
    println!();
    println!("Option C: Use HTTP for local development (not recommended for production)");
    println!("  CONDUCTOR_SERVER_URL=http://localhost:8080/api");
    println!();
    println!("NOTE: Disabling SSL verification is not supported by this SDK for security reasons.");
    println!("      Always use proper certificate management in production environments.");

    // ==========================================================================
    // Summary
    // ==========================================================================
    println!("\n{}", "=".repeat(80));
    println!("CONFIGURATION SUMMARY");
    println!("{}", "=".repeat(80));
    println!();
    println!("Environment Variables:");
    println!("  CONDUCTOR_SERVER_URL        Server API URL");
    println!("  CONDUCTOR_AUTH_KEY          Auth key (Orkes)");
    println!("  CONDUCTOR_AUTH_SECRET       Auth secret (Orkes)");
    println!("  CONDUCTOR_UI_SERVER_URL     UI URL (optional)");
    println!("  CONDUCTOR_TIMEOUT_SECS      Request timeout");
    println!("  CONDUCTOR_AUTH_TOKEN_TTL_MINS  Token TTL");
    println!("  CONDUCTOR_DEBUG             Debug logging");
    println!();
    println!("Builder Methods:");
    println!("  Configuration::new(url)     Create with custom URL");
    println!("  .with_auth(key, secret)     Set authentication");
    println!("  .with_timeout(duration)     Set request timeout");
    println!("  .with_token_ttl(duration)   Set token TTL");
    println!("  .with_debug(true)           Enable debug mode");
    println!();
    println!("Connection configuration example completed!");

    Ok(())
}
