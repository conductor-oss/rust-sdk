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
use conductor::models::{
    AccessType, CreateOrUpdateApplicationRequest, SubjectRef, TargetRef, UpsertGroupRequest,
    UpsertUserRequest,
};

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

    let retrieved = auth.get_application(&app.id).await;

    auth.delete_application(&app.id).await.ok();

    let retrieved = retrieved.expect("get_application should succeed");
    assert_eq!(retrieved.name, app_name);
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

    // Every step's result is collected before any assertion, so a failure still
    // deletes the application instead of orphaning it on the shared server.
    let access_key = auth.create_access_key(&app.id).await;
    let keys = auth.get_access_keys(&app.id).await;
    let toggled = match &access_key {
        Ok(k) => Some(auth.toggle_access_key_status(&app.id, &k.id).await),
        Err(_) => None,
    };
    let deleted = match &access_key {
        Ok(k) => Some(auth.delete_access_key(&app.id, &k.id).await),
        Err(_) => None,
    };

    auth.delete_application(&app.id).await.ok();

    let access_key = access_key.expect("create_access_key should succeed");
    assert!(!access_key.id.is_empty());
    // CreateAccessKeyResponse is {id, secret} server-side -- no `status` field,
    // unlike the AccessKeyResponse returned by the list/toggle endpoints.
    assert!(!access_key.secret.is_empty());

    assert!(!keys.expect("get_access_keys should succeed").is_empty());
    toggled
        .expect("toggle should have been attempted")
        .expect("toggle_access_key_status should succeed");
    deleted
        .expect("delete should have been attempted")
        .expect("delete_access_key should succeed");
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

    let retrieved = auth.get_user(&user_id).await;

    auth.delete_user(&user_id).await.ok();

    assert_eq!(retrieved.expect("get_user should succeed").id, user_id);
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

    let retrieved = auth.get_group(&group_id).await;

    auth.delete_group(&group_id).await.ok();

    assert_eq!(retrieved.expect("get_group should succeed").id, group_id);
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

#[tokio::test]
async fn test_grant_and_remove_permissions() {
    let config = test_config();
    let client = ConductorClient::new(config).unwrap();
    if client.is_oss().await {
        println!("Skipping: Authorization API requires Orkes Enterprise Conductor");
        return;
    }
    let auth = client.authorization_client();
    let metadata = client.metadata_client();

    // A real (freshly created) user id is required here: Enterprise 400s on
    // the empty path segment this used to send (GET /users//permissions).
    let user_id = generate_unique_name("test_perm_user");
    let workflow_name = generate_unique_workflow_name("test_perm_wf");

    auth.upsert_user(
        &UpsertUserRequest::new(format!("Test User {}", &user_id[..8])),
        &user_id,
    )
    .await
    .expect("upsert_user should succeed");

    // Deferred so a panic in the assertions below still tears down the user and
    // the workflow def on the shared server this runs against.
    let outcome = grant_get_remove_flow(&auth, &metadata, &user_id, &workflow_name).await;

    metadata.delete_workflow_def(&workflow_name, 1).await.ok();
    auth.delete_user(&user_id).await.ok();

    outcome.expect("grant -> get -> remove flow should succeed");
}

/// The actual grant -> get -> remove assertions, factored out so the caller can
/// run cleanup unconditionally before surfacing a failure.
///
/// This is what makes `AuthorizationClient::get_granted_permissions_for_user`'s
/// envelope handling load-bearing: the server answers
/// `{"grantedAccess": [{target, access, tag}]}` (Orkes
/// `rest/model/responses/GrantedAccessResponse.java`), and `grantedAccess`
/// carries `#[serde(default)]`, so a test that only ever sees an empty list
/// would pass even if the envelope were parsed wrongly.
async fn grant_get_remove_flow(
    auth: &conductor::client::AuthorizationClient,
    metadata: &conductor::client::MetadataClient,
    user_id: &str,
    workflow_name: &str,
) -> Result<(), String> {
    let workflow_def = conductor::models::WorkflowDef::new(workflow_name)
        .with_version(1)
        .with_task(conductor::models::WorkflowTask::wait("wait_ref"));
    metadata
        .register_workflow_def(&workflow_def)
        .await
        .map_err(|e| format!("register_workflow_def failed: {e:?}"))?;

    let subject = SubjectRef::user(user_id);
    let target = TargetRef::workflow(workflow_name);
    let access = [AccessType::Read, AccessType::Execute];

    auth.grant_permissions(&subject, &target, &access)
        .await
        .map_err(|e| format!("grant_permissions failed: {e:?}"))?;

    let perms = auth
        .get_granted_permissions_for_user(user_id)
        .await
        .map_err(|e| format!("get_granted_permissions_for_user failed: {e:?}"))?;

    let granted = perms
        .iter()
        .find(|p| p.target.as_ref().is_some_and(|t| t.id == workflow_name))
        .ok_or_else(|| {
            format!("granted permission for {workflow_name:?} not found in {perms:?}")
        })?;
    for expected in access {
        if !granted.access.contains(&expected) {
            return Err(format!(
                "expected {expected:?} in granted access {:?}",
                granted.access
            ));
        }
    }

    auth.remove_permissions(&subject, &target, &access)
        .await
        .map_err(|e| format!("remove_permissions failed: {e:?}"))?;

    let after = auth
        .get_granted_permissions_for_user(user_id)
        .await
        .map_err(|e| format!("get_granted_permissions_for_user (after remove) failed: {e:?}"))?;
    if after
        .iter()
        .any(|p| p.target.as_ref().is_some_and(|t| t.id == workflow_name))
    {
        return Err(format!(
            "permission for {workflow_name:?} still present after remove_permissions: {after:?}"
        ));
    }

    Ok(())
}

// NOTE: despite the name, this doesn't actually call check_permissions -- it
// only smoke-tests get_granted_permissions_for_group. The group envelope shares
// its deserialization path with the user one, which
// `test_grant_and_remove_permissions` above exercises for real.
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

    let perms = auth.get_granted_permissions_for_group(&group_id).await;

    auth.delete_group(&group_id).await.ok();

    let perms = perms.expect("get_granted_permissions_for_group should succeed");
    println!("Found {} group permissions", perms.len());
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
