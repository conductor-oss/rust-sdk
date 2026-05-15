// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use std::ops::Deref;
use tracing::{debug, info};

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::MetadataTag;

use super::MetadataClient;

/// Extended metadata client with Orkes-specific features (tagging APIs)
///
/// This client provides all the functionality of `MetadataClient` plus
/// tagging capabilities for workflows and tasks. Use `Deref` to access
/// base methods transparently.
///
/// # Example
///
/// ```rust,ignore
/// let orkes_metadata = client.orkes_metadata_client();
///
/// // Access base methods via Deref
/// orkes_metadata.register_workflow_def(&workflow).await?;
///
/// // Use Orkes-specific tagging methods
/// let tag = MetadataTag::with_value("env", "production");
/// orkes_metadata.add_workflow_tag("my-workflow", &tag).await?;
/// ```
#[derive(Clone)]
pub struct OrkesMetadataClient {
    inner: MetadataClient,
    api: ApiClient,
}

impl Deref for OrkesMetadataClient {
    type Target = MetadataClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl OrkesMetadataClient {
    /// Create a new Orkes metadata client
    pub fn new(api: ApiClient) -> Self {
        Self {
            inner: MetadataClient::new(api.clone()),
            api,
        }
    }

    // ==================== Workflow Tags ====================

    /// Add a tag to a workflow definition
    pub async fn add_workflow_tag(&self, workflow_name: &str, tag: &MetadataTag) -> Result<()> {
        debug!(
            workflow_name = %workflow_name,
            tag_key = %tag.key,
            "Adding workflow tag"
        );

        let path = format!("/metadata/workflow/{}/tags", workflow_name);
        self.api.post_no_response(ApiPath::templated(&path, "/metadata/workflow/{workflowName}/tags"), &[tag]).await?;

        info!(
            workflow_name = %workflow_name,
            tag_key = %tag.key,
            "Workflow tag added"
        );

        Ok(())
    }

    /// Get all tags for a workflow definition
    pub async fn get_workflow_tags(&self, workflow_name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/metadata/workflow/{}/tags", workflow_name);
        self.api.get(ApiPath::templated(&path, "/metadata/workflow/{workflowName}/tags")).await
    }

    /// Set tags for a workflow definition (replaces existing tags)
    pub async fn set_workflow_tags(&self, workflow_name: &str, tags: &[MetadataTag]) -> Result<()> {
        debug!(
            workflow_name = %workflow_name,
            tag_count = tags.len(),
            "Setting workflow tags"
        );

        let path = format!("/metadata/workflow/{}/tags", workflow_name);
        self.api.put_no_response(ApiPath::templated(&path, "/metadata/workflow/{workflowName}/tags"), tags).await?;

        info!(
            workflow_name = %workflow_name,
            tag_count = tags.len(),
            "Workflow tags set"
        );

        Ok(())
    }

    /// Delete a tag from a workflow definition
    pub async fn delete_workflow_tag(&self, workflow_name: &str, tag: &MetadataTag) -> Result<()> {
        debug!(
            workflow_name = %workflow_name,
            tag_key = %tag.key,
            "Deleting workflow tag"
        );

        let path = format!("/metadata/workflow/{}/tags", workflow_name);
        self.api.delete_with_body(ApiPath::templated(&path, "/metadata/workflow/{workflowName}/tags"), &[tag]).await?;

        info!(
            workflow_name = %workflow_name,
            tag_key = %tag.key,
            "Workflow tag deleted"
        );

        Ok(())
    }

    // ==================== Task Tags ====================

    /// Add a tag to a task definition
    pub async fn add_task_tag(&self, task_name: &str, tag: &MetadataTag) -> Result<()> {
        debug!(
            task_name = %task_name,
            tag_key = %tag.key,
            "Adding task tag"
        );

        let path = format!("/metadata/taskdefs/{}/tags", task_name);
        self.api.post_no_response(ApiPath::templated(&path, "/metadata/taskdefs/{taskName}/tags"), &[tag]).await?;

        info!(
            task_name = %task_name,
            tag_key = %tag.key,
            "Task tag added"
        );

        Ok(())
    }

    /// Get all tags for a task definition
    pub async fn get_task_tags(&self, task_name: &str) -> Result<Vec<MetadataTag>> {
        let path = format!("/metadata/taskdefs/{}/tags", task_name);
        self.api.get(ApiPath::templated(&path, "/metadata/taskdefs/{taskName}/tags")).await
    }

    /// Set tags for a task definition (replaces existing tags)
    pub async fn set_task_tags(&self, task_name: &str, tags: &[MetadataTag]) -> Result<()> {
        debug!(
            task_name = %task_name,
            tag_count = tags.len(),
            "Setting task tags"
        );

        let path = format!("/metadata/taskdefs/{}/tags", task_name);
        self.api.put_no_response(ApiPath::templated(&path, "/metadata/taskdefs/{taskName}/tags"), tags).await?;

        info!(
            task_name = %task_name,
            tag_count = tags.len(),
            "Task tags set"
        );

        Ok(())
    }

    /// Delete a tag from a task definition
    pub async fn delete_task_tag(&self, task_name: &str, tag: &MetadataTag) -> Result<()> {
        debug!(
            task_name = %task_name,
            tag_key = %tag.key,
            "Deleting task tag"
        );

        let path = format!("/metadata/taskdefs/{}/tags", task_name);
        self.api.delete_with_body(ApiPath::templated(&path, "/metadata/taskdefs/{taskName}/tags"), &[tag]).await?;

        info!(
            task_name = %task_name,
            tag_key = %tag.key,
            "Task tag deleted"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_orkes_metadata_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = OrkesMetadataClient::new(api);
    }

    #[test]
    fn test_deref_to_metadata_client() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let orkes_client = OrkesMetadataClient::new(api);

        // Access inner MetadataClient via Deref
        let _metadata_ref: &MetadataClient = &orkes_client;
    }
}
