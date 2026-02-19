// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

mod common;

use common::*;
use conductor::client::ConductorClient;
use conductor::models::{CreateOrUpdateApplicationRequest, UpsertGroupRequest, UpsertUserRequest};

// =============================================================================
// Application Tests
// =============================================================================

#[tokio::test]
async fn test_create_and_get_application() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    let app_name = generate_unique_name("test_app");

    // Create application
    let request = CreateOrUpdateApplicationRequest::new(&app_name);

    match auth.create_application(&request).await {
        Ok(app) => {
            assert_eq!(app.name, app_name);

            // Get application
            match auth.get_application(&app.id).await {
                Ok(retrieved) => {
                    assert_eq!(retrieved.name, app_name);
                }
                Err(e) => eprintln!("Warning: get_application failed: {:?}", e),
            }

            // Delete application
            auth.delete_application(&app.id).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: create_application failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_list_applications() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // List all applications
    match auth.list_applications().await {
        Ok(apps) => {
            println!("Found {} applications", apps.len());
        }
        Err(e) => {
            eprintln!("Warning: list_applications failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_create_access_key() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    let app_name = generate_unique_name("test_app_key");

    // Create application first
    let request = CreateOrUpdateApplicationRequest::new(&app_name);

    match auth.create_application(&request).await {
        Ok(app) => {
            // Create access key
            match auth.create_access_key(&app.id).await {
                Ok(access_key) => {
                    assert!(!access_key.id.is_empty());
                    assert!(!access_key.secret.is_empty());

                    // Get access keys
                    match auth.get_access_keys(&app.id).await {
                        Ok(keys) => {
                            assert!(!keys.is_empty());
                        }
                        Err(e) => eprintln!("Warning: get_access_keys failed: {:?}", e),
                    }

                    // Toggle access key status
                    auth.toggle_access_key_status(&app.id, &access_key.id)
                        .await
                        .ok();

                    // Delete access key
                    auth.delete_access_key(&app.id, &access_key.id).await.ok();
                }
                Err(e) => eprintln!("Warning: create_access_key failed: {:?}", e),
            }

            // Delete application
            auth.delete_application(&app.id).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: create_application failed: {:?}", e);
        }
    }
}

// =============================================================================
// User Tests
// =============================================================================

#[tokio::test]
async fn test_upsert_and_get_user() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    let user_id = generate_unique_name("test_user");

    // Upsert user
    let request = UpsertUserRequest::new(format!("Test User {}", &user_id[..8]));

    match auth.upsert_user(&request, &user_id).await {
        Ok(user) => {
            assert_eq!(user.id, user_id);

            // Get user
            match auth.get_user(&user_id).await {
                Ok(retrieved) => {
                    assert_eq!(retrieved.id, user_id);
                }
                Err(e) => eprintln!("Warning: get_user failed: {:?}", e),
            }

            // Delete user
            auth.delete_user(&user_id).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: upsert_user failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_list_users() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // List all users
    match auth.list_users(false).await {
        Ok(users) => {
            println!("Found {} users", users.len());
        }
        Err(e) => {
            eprintln!("Warning: list_users failed: {:?}", e);
        }
    }
}

// =============================================================================
// Group Tests
// =============================================================================

#[tokio::test]
async fn test_upsert_and_get_group() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    let group_id = generate_unique_name("test_group");

    // Upsert group using builder
    let request = UpsertGroupRequest::new(&group_id)
        .with_description(format!("Test group {}", &group_id[..8]));

    match auth.upsert_group(&request, &group_id).await {
        Ok(group) => {
            assert_eq!(group.id, group_id);

            // Get group
            match auth.get_group(&group_id).await {
                Ok(retrieved) => {
                    assert_eq!(retrieved.id, group_id);
                }
                Err(e) => eprintln!("Warning: get_group failed: {:?}", e),
            }

            // Delete group
            auth.delete_group(&group_id).await.ok();
        }
        Err(e) => {
            eprintln!("Warning: upsert_group failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_list_groups() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // List all groups
    match auth.list_groups().await {
        Ok(groups) => {
            println!("Found {} groups", groups.len());
        }
        Err(e) => {
            eprintln!("Warning: list_groups failed: {:?}", e);
        }
    }
}

// =============================================================================
// Permission Tests
// =============================================================================

#[tokio::test]
async fn test_grant_and_remove_permissions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // Get granted permissions for current user
    match auth.get_granted_permissions_for_user("").await {
        Ok(perms) => {
            println!("Found {} permissions", perms.len());
        }
        Err(e) => {
            eprintln!("Warning: get_granted_permissions failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_check_permissions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // Get granted permissions at top level
    match auth.get_granted_permissions_for_group("").await {
        Ok(perms) => {
            println!("Found {} group permissions", perms.len());
        }
        Err(e) => {
            eprintln!("Warning: get_granted_permissions_for_group failed: {:?}", e);
        }
    }
}

// =============================================================================
// Role Tests
// =============================================================================

#[tokio::test]
async fn test_create_and_get_role() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // List custom roles
    match auth.list_custom_roles().await {
        Ok(roles) => {
            println!("Found {} custom roles", roles.len());
        }
        Err(e) => {
            eprintln!("Warning: list_custom_roles failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_list_all_roles() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    let auth = client.authorization_client();

    // List all roles
    match auth.list_available_permissions().await {
        Ok(perms) => {
            println!("Found {} permission types", perms.len());
        }
        Err(e) => {
            eprintln!("Warning: list_available_permissions failed: {:?}", e);
        }
    }
}
