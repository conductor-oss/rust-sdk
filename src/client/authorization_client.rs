// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::{
    AccessKey, AccessType, ConductorApplication, ConductorUser, CreateOrUpdateApplicationRequest,
    CreatedAccessKey, GrantedPermission, Group, MetadataTag, SubjectRef, TargetRef,
    UpsertGroupRequest, UpsertUserRequest,
};
use std::collections::HashMap;

/// Client for authorization operations
#[derive(Clone)]
pub struct AuthorizationClient {
    api: ApiClient,
}

impl AuthorizationClient {
    /// Create a new authorization client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    // ===========================
    // Applications
    // ===========================

    /// Create an application
    pub async fn create_application(
        &self,
        request: &CreateOrUpdateApplicationRequest,
    ) -> Result<ConductorApplication> {
        self.api.post("/applications", request).await
    }

    /// Get an application by ID
    pub async fn get_application(&self, application_id: &str) -> Result<ConductorApplication> {
        let path = format!("/applications/{}", application_id);
        self.api.get(ApiPath::templated(&path, "/applications/{applicationId}")).await
    }

    /// List all applications
    pub async fn list_applications(&self) -> Result<Vec<ConductorApplication>> {
        self.api.get("/applications").await
    }

    /// Update an application
    pub async fn update_application(
        &self,
        request: &CreateOrUpdateApplicationRequest,
        application_id: &str,
    ) -> Result<ConductorApplication> {
        let path = format!("/applications/{}", application_id);
        self.api.put(ApiPath::templated(&path, "/applications/{applicationId}"), request).await
    }

    /// Delete an application
    pub async fn delete_application(&self, application_id: &str) -> Result<()> {
        let path = format!("/applications/{}", application_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/applications/{applicationId}")).await
    }

    /// Get application ID by access key ID
    pub async fn get_app_by_access_key_id(&self, access_key_id: &str) -> Result<String> {
        let path = format!("/applications/key/{}", access_key_id);
        self.api.get(ApiPath::templated(&path, "/applications/key/{accessKeyId}")).await
    }

    /// Add a role to application user
    pub async fn add_role_to_application_user(
        &self,
        application_id: &str,
        role: &str,
    ) -> Result<()> {
        let path = format!("/applications/{}/roles/{}", application_id, role);
        self.api.post_no_body_no_response(ApiPath::templated(&path, "/applications/{applicationId}/roles/{role}")).await
    }

    /// Remove a role from application user
    pub async fn remove_role_from_application_user(
        &self,
        application_id: &str,
        role: &str,
    ) -> Result<()> {
        let path = format!("/applications/{}/roles/{}", application_id, role);
        self.api.delete_no_content(ApiPath::templated(&path, "/applications/{applicationId}/roles/{role}")).await
    }

    /// Set tags for an application
    pub async fn set_application_tags(
        &self,
        tags: &[MetadataTag],
        application_id: &str,
    ) -> Result<()> {
        let path = format!("/applications/{}/tags", application_id);
        self.api.put_no_response(ApiPath::templated(&path, "/applications/{applicationId}/tags"), tags).await
    }

    /// Get tags for an application
    pub async fn get_application_tags(&self, application_id: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/applications/{}/tags", application_id);
        self.api.get(ApiPath::templated(&path, "/applications/{applicationId}/tags")).await
    }

    /// Delete tags from an application
    pub async fn delete_application_tags(
        &self,
        tags: &[MetadataTag],
        application_id: &str,
    ) -> Result<()> {
        let path = format!("/applications/{}/tags", application_id);
        self.api.delete_with_body(ApiPath::templated(&path, "/applications/{applicationId}/tags"), tags).await
    }

    /// Create an access key for an application
    pub async fn create_access_key(&self, application_id: &str) -> Result<CreatedAccessKey> {
        let path = format!("/applications/{}/accessKeys", application_id);
        self.api.post_no_body(ApiPath::templated(&path, "/applications/{applicationId}/accessKeys")).await
    }

    /// Get access keys for an application
    pub async fn get_access_keys(&self, application_id: &str) -> Result<Vec<AccessKey>> {
        let path = format!("/applications/{}/accessKeys", application_id);
        self.api.get(ApiPath::templated(&path, "/applications/{applicationId}/accessKeys")).await
    }

    /// Toggle the status of an access key
    pub async fn toggle_access_key_status(
        &self,
        application_id: &str,
        key_id: &str,
    ) -> Result<AccessKey> {
        let path = format!(
            "/applications/{}/accessKeys/{}/status",
            application_id, key_id
        );
        self.api.post_no_body(ApiPath::templated(&path, "/applications/{applicationId}/accessKeys/{keyId}/status")).await
    }

    /// Delete an access key
    pub async fn delete_access_key(&self, application_id: &str, key_id: &str) -> Result<()> {
        let path = format!("/applications/{}/accessKeys/{}", application_id, key_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/applications/{applicationId}/accessKeys/{keyId}")).await
    }

    // ===========================
    // Users
    // ===========================

    /// Create or update a user
    pub async fn upsert_user(
        &self,
        request: &UpsertUserRequest,
        user_id: &str,
    ) -> Result<ConductorUser> {
        let path = format!("/users/{}", user_id);
        self.api.put(ApiPath::templated(&path, "/users/{userId}"), request).await
    }

    /// Get a user by ID
    pub async fn get_user(&self, user_id: &str) -> Result<ConductorUser> {
        let path = format!("/users/{}", user_id);
        self.api.get(ApiPath::templated(&path, "/users/{userId}")).await
    }

    /// List all users
    pub async fn list_users(&self, apps: bool) -> Result<Vec<ConductorUser>> {
        if apps {
            self.api.get_with_params("/users", &[("apps", "true")]).await
        } else {
            self.api.get("/users").await
        }
    }

    /// Delete a user
    pub async fn delete_user(&self, user_id: &str) -> Result<()> {
        let path = format!("/users/{}", user_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/users/{userId}")).await
    }

    /// Get permissions granted to a user
    pub async fn get_granted_permissions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<GrantedPermission>> {
        let path = format!("/users/{}/permissions", user_id);
        self.api.get(ApiPath::templated(&path, "/users/{userId}/permissions")).await
    }

    /// Check if user has permissions over a target
    pub async fn check_permissions(
        &self,
        user_id: &str,
        target_type: &str,
        target_id: &str,
    ) -> Result<HashMap<String, bool>> {
        let path = format!(
            "/users/{}/permissions/{}/{}",
            user_id, target_type, target_id
        );
        self.api.get(ApiPath::templated(&path, "/users/{userId}/permissions/{targetType}/{targetId}")).await
    }

    // ===========================
    // Groups
    // ===========================

    /// Create or update a group
    pub async fn upsert_group(
        &self,
        request: &UpsertGroupRequest,
        group_id: &str,
    ) -> Result<Group> {
        let path = format!("/groups/{}", group_id);
        self.api.put(ApiPath::templated(&path, "/groups/{groupId}"), request).await
    }

    /// Get a group by ID
    pub async fn get_group(&self, group_id: &str) -> Result<Group> {
        let path = format!("/groups/{}", group_id);
        self.api.get(ApiPath::templated(&path, "/groups/{groupId}")).await
    }

    /// List all groups
    pub async fn list_groups(&self) -> Result<Vec<Group>> {
        self.api.get("/groups").await
    }

    /// Delete a group
    pub async fn delete_group(&self, group_id: &str) -> Result<()> {
        let path = format!("/groups/{}", group_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/groups/{groupId}")).await
    }

    /// Get permissions granted to a group
    pub async fn get_granted_permissions_for_group(
        &self,
        group_id: &str,
    ) -> Result<Vec<GrantedPermission>> {
        let path = format!("/groups/{}/permissions", group_id);
        self.api.get(ApiPath::templated(&path, "/groups/{groupId}/permissions")).await
    }

    /// Add a user to a group
    pub async fn add_user_to_group(&self, group_id: &str, user_id: &str) -> Result<()> {
        let path = format!("/groups/{}/users/{}", group_id, user_id);
        self.api.post_no_body_no_response(ApiPath::templated(&path, "/groups/{groupId}/users/{userId}")).await
    }

    /// Add multiple users to a group
    pub async fn add_users_to_group(&self, group_id: &str, user_ids: &[String]) -> Result<()> {
        let path = format!("/groups/{}/users", group_id);
        self.api.post_no_response(ApiPath::templated(&path, "/groups/{groupId}/users"), user_ids).await
    }

    /// Get all users in a group
    pub async fn get_users_in_group(&self, group_id: &str) -> Result<Vec<ConductorUser>> {
        let path = format!("/groups/{}/users", group_id);
        self.api.get(ApiPath::templated(&path, "/groups/{groupId}/users")).await
    }

    /// Remove a user from a group
    pub async fn remove_user_from_group(&self, group_id: &str, user_id: &str) -> Result<()> {
        let path = format!("/groups/{}/users/{}", group_id, user_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/groups/{groupId}/users/{userId}")).await
    }

    /// Remove multiple users from a group
    pub async fn remove_users_from_group(&self, group_id: &str, user_ids: &[String]) -> Result<()> {
        let path = format!("/groups/{}/users", group_id);
        self.api.delete_with_body(ApiPath::templated(&path, "/groups/{groupId}/users"), user_ids).await
    }

    // ===========================
    // Permissions
    // ===========================

    /// Grant permissions to a subject over a target
    pub async fn grant_permissions(
        &self,
        subject: &SubjectRef,
        target: &TargetRef,
        access: &[AccessType],
    ) -> Result<()> {
        let body = serde_json::json!({
            "subject": subject,
            "target": target,
            "access": access
        });
        self.api.post_no_response("/auth/authorization", &body).await
    }

    /// Get permissions for a target
    pub async fn get_permissions(
        &self,
        target: &TargetRef,
    ) -> Result<HashMap<String, Vec<SubjectRef>>> {
        // Serialize target_type and extract the string value without quotes
        let target_type_str = serde_json::to_string(&target.target_type)?;
        let path = format!(
            "/auth/authorization/{}/{}",
            target_type_str.trim_matches('"'),
            target.id
        );
        self.api.get(ApiPath::templated(&path, "/auth/authorization/{targetType}/{targetId}")).await
    }

    /// Remove permissions from a subject over a target
    pub async fn remove_permissions(
        &self,
        subject: &SubjectRef,
        target: &TargetRef,
        access: &[AccessType],
    ) -> Result<()> {
        let body = serde_json::json!({
            "subject": subject,
            "target": target,
            "access": access
        });
        self.api.delete_with_body("/auth/authorization", &body).await
    }

    // ===========================
    // Roles
    // ===========================

    /// List all roles
    pub async fn list_all_roles(&self) -> Result<Vec<serde_json::Value>> {
        self.api.get("/roles").await
    }

    /// List system roles
    pub async fn list_system_roles(&self) -> Result<HashMap<String, serde_json::Value>> {
        self.api.get("/roles/system").await
    }

    /// List custom roles
    pub async fn list_custom_roles(&self) -> Result<Vec<serde_json::Value>> {
        self.api.get("/roles/custom").await
    }

    /// List available permissions
    pub async fn list_available_permissions(&self) -> Result<HashMap<String, serde_json::Value>> {
        self.api.get("/roles/permissions").await
    }

    /// Create a custom role
    pub async fn create_role(&self, request: &serde_json::Value) -> Result<serde_json::Value> {
        self.api.post("/roles", request).await
    }

    /// Get a role by name
    pub async fn get_role(&self, role_name: &str) -> Result<serde_json::Value> {
        let path = format!("/roles/{}", role_name);
        self.api.get(ApiPath::templated(&path, "/roles/{roleName}")).await
    }

    /// Update a custom role
    pub async fn update_role(
        &self,
        role_name: &str,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let path = format!("/roles/{}", role_name);
        self.api.put(ApiPath::templated(&path, "/roles/{roleName}"), request).await
    }

    /// Delete a custom role
    pub async fn delete_role(&self, role_name: &str) -> Result<()> {
        let path = format!("/roles/{}", role_name);
        self.api.delete_no_content(ApiPath::templated(&path, "/roles/{roleName}")).await
    }

    // ===========================
    // Token / User Info
    // ===========================

    /// Get user info from the current token
    pub async fn get_user_info_from_token(&self) -> Result<serde_json::Value> {
        self.api.get("/auth/userInfo").await
    }

    /// Generate a token using access key credentials
    pub async fn generate_token(
        &self,
        key_id: &str,
        key_secret: &str,
    ) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "keyId": key_id,
            "keySecret": key_secret
        });
        self.api.post("/token", &body).await
    }

    // ===========================
    // API Gateway Authentication Config
    // ===========================

    /// Create API Gateway authentication configuration
    pub async fn create_gateway_auth_config(
        &self,
        auth_config: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.api.post("/api-gateway/auth-config", auth_config).await
    }

    /// Get API Gateway authentication configuration by ID
    pub async fn get_gateway_auth_config(&self, config_id: &str) -> Result<serde_json::Value> {
        let path = format!("/api-gateway/auth-config/{}", config_id);
        self.api.get(ApiPath::templated(&path, "/api-gateway/auth-config/{configId}")).await
    }

    /// List all API Gateway authentication configurations
    pub async fn list_gateway_auth_configs(&self) -> Result<Vec<serde_json::Value>> {
        self.api.get("/api-gateway/auth-config").await
    }

    /// Update API Gateway authentication configuration
    pub async fn update_gateway_auth_config(
        &self,
        config_id: &str,
        auth_config: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let path = format!("/api-gateway/auth-config/{}", config_id);
        self.api.put(ApiPath::templated(&path, "/api-gateway/auth-config/{configId}"), auth_config).await
    }

    /// Delete API Gateway authentication configuration
    pub async fn delete_gateway_auth_config(&self, config_id: &str) -> Result<()> {
        let path = format!("/api-gateway/auth-config/{}", config_id);
        self.api.delete_no_content(ApiPath::templated(&path, "/api-gateway/auth-config/{configId}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_authorization_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = AuthorizationClient::new(api);
    }
}
