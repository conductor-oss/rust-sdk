//! Authorization Example
//!
//! Demonstrates user, group, and permission management.
//!
//! What it shows:
//! - User management (create, update, delete)
//! - Group management
//! - Adding users to groups
//! - Granting and revoking permissions
//! - Application and access key management
//!
//! Run with: cargo run --example authorization_example
//!
//! Prerequisites:
//! - Conductor server running on localhost:8080 with authentication enabled
//! - Set CONDUCTOR_SERVER_URL, CONDUCTOR_AUTH_KEY, CONDUCTOR_AUTH_SECRET

use conductor::{
    client::ConductorClient,
    configuration::Configuration,
    error::Result,
    models::{
        AccessType, CreateOrUpdateApplicationRequest, MetadataTag, SubjectRef, SubjectType,
        TargetRef, TargetType, UpsertGroupRequest, UpsertUserRequest,
    },
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
    let auth = client.authorization_client();

    // ==============================
    // Get Current User Info
    // ==============================
    info!("\n=== Current User Info ===");

    match auth.get_user_info_from_token().await {
        Ok(user_info) => {
            info!("Current user info: {:?}", user_info);
        }
        Err(e) => {
            info!("Could not get user info (may need authentication): {}", e);
        }
    }

    // ==============================
    // List Available Roles
    // ==============================
    info!("\n=== Available Roles ===");

    match auth.list_all_roles().await {
        Ok(roles) => {
            info!("Available roles: {}", roles.len());
            for role in roles.iter().take(5) {
                info!("  - {:?}", role);
            }
        }
        Err(e) => {
            info!("Could not list roles: {}", e);
        }
    }

    // ==============================
    // User Management
    // ==============================
    info!("\n=== User Management ===");

    let user_id = format!("rust_demo_user_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let user_request = UpsertUserRequest::new(format!("Demo User {}", &user_id[..8]))
        .with_roles(vec!["USER".to_string()]);

    match auth.upsert_user(&user_request, &user_id).await {
        Ok(user) => {
            info!("Created user: {:?}", user.id);

            // Get user
            match auth.get_user(&user_id).await {
                Ok(retrieved) => {
                    info!("Retrieved user: {:?}", retrieved.name);
                }
                Err(e) => info!("Could not retrieve user: {}", e),
            }

            // List all users
            match auth.list_users(false).await {
                Ok(users) => {
                    info!("Total users: {}", users.len());
                }
                Err(e) => info!("Could not list users: {}", e),
            }
        }
        Err(e) => {
            info!("Could not create user (may need admin permissions): {}", e);
        }
    }

    // ==============================
    // Group Management
    // ==============================
    info!("\n=== Group Management ===");

    let group_id = format!("rust_demo_group_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let group_request = UpsertGroupRequest::new(&group_id)
        .with_description("Demo group created by Rust SDK")
        .with_roles(vec!["USER".to_string()]);

    match auth.upsert_group(&group_request, &group_id).await {
        Ok(group) => {
            info!("Created group: {:?}", group.id);

            // Add user to group
            match auth.add_user_to_group(&group_id, &user_id).await {
                Ok(_) => info!("Added user to group"),
                Err(e) => info!("Could not add user to group: {}", e),
            }

            // Get users in group
            match auth.get_users_in_group(&group_id).await {
                Ok(users) => {
                    info!("Users in group: {}", users.len());
                }
                Err(e) => info!("Could not get users in group: {}", e),
            }

            // List all groups
            match auth.list_groups().await {
                Ok(groups) => {
                    info!("Total groups: {}", groups.len());
                }
                Err(e) => info!("Could not list groups: {}", e),
            }
        }
        Err(e) => {
            info!("Could not create group: {}", e);
        }
    }

    // ==============================
    // Application Management
    // ==============================
    info!("\n=== Application Management ===");

    let app_name = format!("rust_demo_app_{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let app_request = CreateOrUpdateApplicationRequest {
        name: app_name.clone(),
    };

    match auth.create_application(&app_request).await {
        Ok(app) => {
            info!("Created application: {:?}", app.id);

            // Create access key
            let app_id = &app.id;

            // Create access key
            match auth.create_access_key(app_id).await {
                Ok(key) => {
                    info!("Created access key: {:?}", key.id);
                    // Note: key.secret is only shown once!

                    // List access keys
                    match auth.get_access_keys(app_id).await {
                        Ok(keys) => {
                            info!("Total access keys: {}", keys.len());
                        }
                        Err(e) => info!("Could not list access keys: {}", e),
                    }

                    // Toggle key status
                    let key_id = &key.id;
                    match auth.toggle_access_key_status(app_id, key_id).await {
                        Ok(toggled) => {
                            info!("Toggled key status: {:?}", toggled.status);
                        }
                        Err(e) => info!("Could not toggle key: {}", e),
                    }

                    // Delete access key
                    match auth.delete_access_key(app_id, key_id).await {
                        Ok(_) => info!("Deleted access key"),
                        Err(e) => info!("Could not delete access key: {}", e),
                    }
                }
                Err(e) => info!("Could not create access key: {}", e),
            }

            // Add tags to application
            let tags = vec![
                MetadataTag::with_value("environment", "demo"),
                MetadataTag::with_value("created_by", "rust-sdk"),
            ];
            match auth.set_application_tags(&tags, app_id).await {
                Ok(_) => info!("Added tags to application"),
                Err(e) => info!("Could not add tags: {}", e),
            }

            // Delete application
            match auth.delete_application(app_id).await {
                Ok(_) => info!("Deleted application"),
                Err(e) => info!("Could not delete application: {}", e),
            }
        }
        Err(e) => {
            info!("Could not create application: {}", e);
        }
    }

    // ==============================
    // Permission Management
    // ==============================
    info!("\n=== Permission Management ===");

    // Grant permission to user over a workflow
    let subject = SubjectRef {
        id: user_id.clone(),
        subject_type: SubjectType::User,
    };

    let target = TargetRef {
        id: "test_workflow".to_string(),
        target_type: TargetType::WorkflowDef,
    };

    let access = vec![AccessType::Execute, AccessType::Read];

    match auth.grant_permissions(&subject, &target, &access).await {
        Ok(_) => {
            info!("Granted permissions to user over workflow");

            // Get permissions for target
            match auth.get_permissions(&target).await {
                Ok(perms) => {
                    info!("Permissions for target: {:?}", perms);
                }
                Err(e) => info!("Could not get permissions: {}", e),
            }

            // Check user's permissions
            match auth
                .check_permissions(&user_id, "WORKFLOW", "test_workflow")
                .await
            {
                Ok(result) => {
                    info!("User permission check: {:?}", result);
                }
                Err(e) => info!("Could not check permissions: {}", e),
            }

            // Remove permissions
            match auth.remove_permissions(&subject, &target, &access).await {
                Ok(_) => info!("Removed permissions"),
                Err(e) => info!("Could not remove permissions: {}", e),
            }
        }
        Err(e) => {
            info!("Could not grant permissions: {}", e);
        }
    }

    // ==============================
    // Cleanup
    // ==============================
    info!("\n=== Cleanup ===");

    // Remove user from group and delete
    match auth.remove_user_from_group(&group_id, &user_id).await {
        Ok(_) => info!("Removed user from group"),
        Err(e) => info!("Could not remove user from group: {}", e),
    }

    match auth.delete_group(&group_id).await {
        Ok(_) => info!("Deleted group"),
        Err(e) => info!("Could not delete group: {}", e),
    }

    match auth.delete_user(&user_id).await {
        Ok(_) => info!("Deleted user"),
        Err(e) => info!("Could not delete user: {}", e),
    }

    info!("\nAuthorization example completed!");
    Ok(())
}
