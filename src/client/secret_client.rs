// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::MetadataTag;
use std::collections::HashSet;

/// Client for managing secrets
#[derive(Clone)]
pub struct SecretClient {
    api: ApiClient,
}

impl SecretClient {
    /// Create a new secret client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Store a secret
    pub async fn put_secret(&self, key: &str, value: &str) -> Result<()> {
        let path = format!("/secrets/{}", key);
        self.api
            .put_raw(ApiPath::templated(&path, "/secrets/{key}"), value)
            .await
    }

    /// Get a secret value
    ///
    /// The server returns this as `text/plain` (the raw secret value, not a
    /// JSON string), so this uses `get_text` rather than the generic JSON
    /// `get` -- see `ApiClient::get_text`.
    pub async fn get_secret(&self, key: &str) -> Result<String> {
        let path = format!("/secrets/{}", key);
        self.api
            .get_text(ApiPath::templated(&path, "/secrets/{key}"))
            .await
    }

    /// List all secret names
    ///
    /// This is `POST /secrets`, not `GET`. Both server families map the two
    /// verbs to different operations: `POST` is the primary "list all secret
    /// names" endpoint (`SecretController.listAllNames` on OSS,
    /// `SecretResource.listAllSecretNames` on Orkes), while `GET` returns only
    /// the names the caller has access to. On OSS the two happen to agree
    /// because there is no RBAC, which is why the wrong verb went unnoticed.
    pub async fn list_all_secret_names(&self) -> Result<HashSet<String>> {
        let names: Vec<String> = self.api.post_no_body("/secrets").await?;
        Ok(names.into_iter().collect())
    }

    /// List secrets the caller has access to
    ///
    /// `GET /secrets`. Orkes filters by the `access` query parameter (defaulting
    /// to `READ`, with AND semantics across multiple values); OSS has no RBAC and
    /// returns every name.
    pub async fn list_secrets_that_user_can_grant_access_to(&self) -> Result<Vec<String>> {
        self.api.get("/secrets").await
    }

    /// Delete a secret
    pub async fn delete_secret(&self, key: &str) -> Result<()> {
        let path = format!("/secrets/{}", key);
        self.api
            .delete_no_content(ApiPath::templated(&path, "/secrets/{key}"))
            .await
    }

    /// Check if a secret exists
    pub async fn secret_exists(&self, key: &str) -> Result<bool> {
        let path = format!("/secrets/{}/exists", key);
        self.api
            .get(ApiPath::templated(&path, "/secrets/{key}/exists"))
            .await
    }

    /// Set tags for a secret
    pub async fn set_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{}/tags", key);
        self.api
            .put_no_response(ApiPath::templated(&path, "/secrets/{key}/tags"), tags)
            .await
    }

    /// Get tags for a secret
    pub async fn get_secret_tags(&self, key: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/secrets/{}/tags", key);
        self.api
            .get(ApiPath::templated(&path, "/secrets/{key}/tags"))
            .await
    }

    /// Delete tags from a secret
    pub async fn delete_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{}/tags", key);
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
