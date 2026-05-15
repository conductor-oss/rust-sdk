// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::SchemaDef;

/// Client for managing schema definitions
#[derive(Clone)]
pub struct SchemaClient {
    api: ApiClient,
}

impl SchemaClient {
    /// Create a new schema client
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Register a new schema
    pub async fn register_schema(&self, schema: &SchemaDef) -> Result<()> {
        self.api.post_no_response("/schema", schema).await
    }

    /// Get a schema by name and version
    pub async fn get_schema(&self, schema_name: &str, version: i32) -> Result<SchemaDef> {
        let path = format!("/schema/{}", schema_name);
        self.api
            .get_with_params(
                ApiPath::templated(&path, "/schema/{schemaName}"),
                &[("version", &version.to_string())],
            )
            .await
    }

    /// Get all schemas
    pub async fn get_all_schemas(&self) -> Result<Vec<SchemaDef>> {
        self.api.get("/schema").await
    }

    /// Delete a schema by name and version
    pub async fn delete_schema(&self, schema_name: &str, version: i32) -> Result<()> {
        let path = format!("/schema/{}", schema_name);
        self.api
            .delete_with_params(
                ApiPath::templated(&path, "/schema/{schemaName}"),
                &[("version", &version.to_string())],
            )
            .await
    }

    /// Delete all versions of a schema by name
    pub async fn delete_schema_by_name(&self, schema_name: &str) -> Result<()> {
        let path = format!("/schema/{}/all", schema_name);
        self.api
            .delete_no_content(ApiPath::templated(&path, "/schema/{schemaName}/all"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_schema_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = SchemaClient::new(api);
    }
}
