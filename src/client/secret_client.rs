//! Secret client for managing secrets

use crate::error::Result;
use crate::http::ApiClient;
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
        self.api.put_raw(&path, value).await
    }

    /// Get a secret value
    pub async fn get_secret(&self, key: &str) -> Result<String> {
        let path = format!("/secrets/{}", key);
        self.api.get(&path).await
    }

    /// List all secret names
    pub async fn list_all_secret_names(&self) -> Result<HashSet<String>> {
        let path = "/secrets";
        let names: Vec<String> = self.api.get(path).await?;
        Ok(names.into_iter().collect())
    }

    /// List secrets that the user can grant access to
    pub async fn list_secrets_that_user_can_grant_access_to(&self) -> Result<Vec<String>> {
        let path = "/secrets";
        self.api
            .get_with_params(path, &[("grantable", "true")])
            .await
    }

    /// Delete a secret
    pub async fn delete_secret(&self, key: &str) -> Result<()> {
        let path = format!("/secrets/{}", key);
        self.api.delete_no_content(&path).await
    }

    /// Check if a secret exists
    pub async fn secret_exists(&self, key: &str) -> Result<bool> {
        let path = format!("/secrets/{}/exists", key);
        self.api.get(&path).await
    }

    /// Set tags for a secret
    pub async fn set_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{}/tags", key);
        self.api.put_no_response(&path, tags).await
    }

    /// Get tags for a secret
    pub async fn get_secret_tags(&self, key: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/secrets/{}/tags", key);
        self.api.get(&path).await
    }

    /// Delete tags from a secret
    pub async fn delete_secret_tags(&self, tags: &[MetadataTag], key: &str) -> Result<()> {
        let path = format!("/secrets/{}/tags", key);
        self.api.delete_with_body(&path, tags).await
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
