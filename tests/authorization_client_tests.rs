// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.
//
// The whole Authorization surface (applications, users, groups, permissions,
// roles) is not implemented by plain OSS Conductor -- confirmed empirically:
// every call below returns 404 "No static resource api/...". Every test skips
// explicitly via `is_oss()` instead of silently swallowing the resulting
// error the way this file used to; against real Orkes Enterprise (is_oss()
// == false) these now assert for real.

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
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let app_name = generate_unique_name("test_app");
    let request = CreateOrUpdateApplicationRequest::new(&app_name);

    let app = auth
        .create_application(&request)
        .await
        .expect("create_application should succeed");
    assert_eq!(app.name, app_name);

    let retrieved = auth
        .get_application(&app.id)
        .await
        .expect("get_application should succeed");
    assert_eq!(retrieved.name, app_name);

    auth.delete_application(&app.id).await.ok();
}

#[tokio::test]
async fn test_list_applications() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let apps = auth
        .list_applications()
        .await
        .expect("list_applications should succeed");
    println!("Found {} applications", apps.len());
}

#[tokio::test]
async fn test_create_access_key() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let app_name = generate_unique_name("test_app_key");
    let request = CreateOrUpdateApplicationRequest::new(&app_name);

    let app = auth
        .create_application(&request)
        .await
        .expect("create_application should succeed");

    let access_key = auth
        .create_access_key(&app.id)
        .await
        .expect("create_access_key should succeed");
    assert!(!access_key.id.is_empty());
    assert!(!access_key.secret.is_empty());

    let keys = auth
        .get_access_keys(&app.id)
        .await
        .expect("get_access_keys should succeed");
    assert!(!keys.is_empty());

    auth.toggle_access_key_status(&app.id, &access_key.id)
        .await
        .expect("toggle_access_key_status should succeed");
    auth.delete_access_key(&app.id, &access_key.id)
        .await
        .expect("delete_access_key should succeed");

    auth.delete_application(&app.id).await.ok();
}

// =============================================================================
// User Tests
// =============================================================================

#[tokio::test]
async fn test_upsert_and_get_user() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let user_id = generate_unique_name("test_user");
    let request = UpsertUserRequest::new(format!("Test User {}", &user_id[..8]));

    let user = auth
        .upsert_user(&request, &user_id)
        .await
        .expect("upsert_user should succeed");
    assert_eq!(user.id, user_id);

    let retrieved = auth
        .get_user(&user_id)
        .await
        .expect("get_user should succeed");
    assert_eq!(retrieved.id, user_id);

    auth.delete_user(&user_id).await.ok();
}

#[tokio::test]
async fn test_list_users() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let users = auth
        .list_users(false)
        .await
        .expect("list_users should succeed");
    println!("Found {} users", users.len());
}

// =============================================================================
// Group Tests
// =============================================================================

#[tokio::test]
async fn test_upsert_and_get_group() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let group_id = generate_unique_name("test_group");
    let request = UpsertGroupRequest::new(&group_id)
        .with_description(format!("Test group {}", &group_id[..8]));

    let group = auth
        .upsert_group(&request, &group_id)
        .await
        .expect("upsert_group should succeed");
    assert_eq!(group.id, group_id);

    let retrieved = auth
        .get_group(&group_id)
        .await
        .expect("get_group should succeed");
    assert_eq!(retrieved.id, group_id);

    auth.delete_group(&group_id).await.ok();
}

#[tokio::test]
async fn test_list_groups() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let groups = auth
        .list_groups()
        .await
        .expect("list_groups should succeed");
    println!("Found {} groups", groups.len());
}

// =============================================================================
// Permission Tests
// =============================================================================

// NOTE: despite the name, this doesn't actually call grant_permissions /
// remove_permissions -- it only smoke-tests get_granted_permissions_for_user.
// TODO: exercise the full grant -> get -> remove flow with a real TargetRef.
#[tokio::test]
async fn test_grant_and_remove_permissions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    // A real (freshly created) user id is required here: Enterprise 400s on
    // the empty path segment this used to send (GET /users//permissions).
    let user_id = generate_unique_name("test_perm_user");
    auth.upsert_user(
        &UpsertUserRequest::new(format!("Test User {}", &user_id[..8])),
        &user_id,
    )
    .await
    .expect("upsert_user should succeed");

    let perms = auth
        .get_granted_permissions_for_user(&user_id)
        .await
        .expect("get_granted_permissions_for_user should succeed");
    println!("Found {} permissions", perms.len());

    auth.delete_user(&user_id).await.ok();
}

// NOTE: despite the name, this doesn't actually call check_permissions -- it
// only smoke-tests get_granted_permissions_for_group.
// TODO: exercise the full grant -> get -> remove flow with a real TargetRef.
#[tokio::test]
async fn test_check_permissions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    // A real (freshly created) group id is required here: Enterprise 400s on
    // the empty path segment this used to send (GET /groups//permissions).
    let group_id = generate_unique_name("test_perm_group");
    let request = UpsertGroupRequest::new(&group_id)
        .with_description(format!("Test group {}", &group_id[..8]));
    auth.upsert_group(&request, &group_id)
        .await
        .expect("upsert_group should succeed");

    let perms = auth
        .get_granted_permissions_for_group(&group_id)
        .await
        .expect("get_granted_permissions_for_group should succeed");
    println!("Found {} group permissions", perms.len());

    auth.delete_group(&group_id).await.ok();
}

// =============================================================================
// Role Tests
// =============================================================================

#[tokio::test]
async fn test_create_and_get_role() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let roles = auth
        .list_custom_roles()
        .await
        .expect("list_custom_roles should succeed");
    println!("Found {} custom roles", roles.len());
}

#[tokio::test]
async fn test_list_all_roles() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();

    let perms = auth
        .list_available_permissions()
        .await
        .expect("list_available_permissions should succeed");
    println!("Found {} permission types", perms.len());
}
