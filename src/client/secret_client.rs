// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::MetadataTag;
use std::collections::HashSet;

/// Client for managing secrets.
#[derive(Clone)]
pub struct SecretClient {
    api: ApiClient,
}

impl SecretClient {
    /// Create a new secret client.
    #[must_use]
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Store a secret.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn put_secret(&self, key: &str, value: &str) -> Result<()> {
        let path = format!("/secrets/{key}");
        self.api
            .put_raw(ApiPath::templated(&path, "/secrets/{key}"), value)
            .await
    }

    /// Get a secret value.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_secret(&self, key: &str) -> Result<String> {
        let path = format!("/secrets/{key}");
        self.api
            .get(ApiPath::templated(&path, "/secrets/{key}"))
            .await
    }

    /// List all secret names.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn list_all_secret_names(&self) -> Result<HashSet<String>> {
        let names: Vec<String> = self.api.get("/secrets").await?;
        Ok(names.into_iter().collect())
    }

    /// List secrets that the user can grant access to.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn list_secrets_that_user_can_grant_access_to(&self) -> Result<Vec<String>> {
        self.api
            .get_with_params("/secrets", &[("grantable", "true")])
            .await
    }

    /// Delete a secret.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn delete_secret(&self, key: &str) -> Result<()> {
        let path = format!("/secrets/{key}");
        self.api
            .delete_no_content(ApiPath::templated(&path, "/secrets/{key}"))
            .await
    }

    /// Check if a secret exists.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn secret_exists(&self, key: &str) -> Result<bool> {
        let path = format!("/secrets/{key}/exists");
        self.api
            .get(ApiPath::templated(&path, "/secrets/{key}/exists"))
            .await
    }

    /// Set tags for a secret.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn set_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{key}/tags");
        self.api
            .put_no_response(ApiPath::templated(&path, "/secrets/{key}/tags"), tags)
            .await
    }

    /// Get tags for a secret.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_secret_tags(&self, key: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/secrets/{key}/tags");
        self.api
            .get(ApiPath::templated(&path, "/secrets/{key}/tags"))
            .await
    }

    /// Delete tags from a secret.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn delete_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{key}/tags");
        self.api
            .delete_with_body(ApiPath::templated(&path, "/secrets/{key}/tags"), tags)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_secret_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = SecretClient::new(api);
    }
}
