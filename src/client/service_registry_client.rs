// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use crate::error::Result;
use crate::http::{ApiClient, ApiPath};
use crate::models::{
    CircuitBreakerTransitionResponse, ProtoRegistryEntry, ServiceMethod, ServiceRegistry,
};

/// Client for managing the service registry: HTTP/gRPC service definitions,
/// their methods, proto files, and circuit breakers.
#[derive(Clone)]
pub struct ServiceRegistryClient {
    api: ApiClient,
}

impl ServiceRegistryClient {
    /// Create a new service registry client.
    #[must_use]
    pub fn new(api: ApiClient) -> Self {
        Self { api }
    }

    /// Get all registered services.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_registered_services(&self) -> Result<Vec<ServiceRegistry>> {
        self.api.get("/registry/service").await
    }

    /// Get a single registered service by name.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_service(&self, name: &str) -> Result<ServiceRegistry> {
        let path = format!("/registry/service/{name}");
        self.api
            .get(ApiPath::templated(&path, "/registry/service/{name}"))
            .await
    }

    /// Register a new service, or update an existing one with the same name.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn add_or_update_service(&self, service_registry: &ServiceRegistry) -> Result<()> {
        self.api
            .post_no_response("/registry/service", service_registry)
            .await
    }

    /// Remove a registered service.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn remove_service(&self, name: &str) -> Result<()> {
        let path = format!("/registry/service/{name}");
        self.api
            .delete_no_content(ApiPath::templated(&path, "/registry/service/{name}"))
            .await
    }

    /// Open (trip) a service's circuit breaker.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn open_circuit_breaker(
        &self,
        name: &str,
    ) -> Result<CircuitBreakerTransitionResponse> {
        let path = format!("/registry/service/{name}/circuit-breaker/open");
        self.api
            .post_no_body(ApiPath::templated(
                &path,
                "/registry/service/{name}/circuit-breaker/open",
            ))
            .await
    }

    /// Close (reset) a service's circuit breaker.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn close_circuit_breaker(
        &self,
        name: &str,
    ) -> Result<CircuitBreakerTransitionResponse> {
        let path = format!("/registry/service/{name}/circuit-breaker/close");
        self.api
            .post_no_body(ApiPath::templated(
                &path,
                "/registry/service/{name}/circuit-breaker/close",
            ))
            .await
    }

    /// Get the current status of a service's circuit breaker.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_circuit_breaker_status(
        &self,
        name: &str,
    ) -> Result<CircuitBreakerTransitionResponse> {
        let path = format!("/registry/service/{name}/circuit-breaker/status");
        self.api
            .get(ApiPath::templated(
                &path,
                "/registry/service/{name}/circuit-breaker/status",
            ))
            .await
    }

    /// Convenience wrapper around [`get_circuit_breaker_status`](Self::get_circuit_breaker_status)
    /// that just checks whether the breaker is currently open.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn is_circuit_breaker_open(&self, name: &str) -> Result<bool> {
        Ok(self.get_circuit_breaker_status(name).await?.is_open())
    }

    /// Add a new method to a registered service, or update an existing one with
    /// the same operation name.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn add_or_update_method(
        &self,
        registry_name: &str,
        method: &ServiceMethod,
    ) -> Result<()> {
        let path = format!("/registry/service/{registry_name}/methods");
        self.api
            .post_no_response(
                ApiPath::templated(&path, "/registry/service/{registryName}/methods"),
                method,
            )
            .await
    }

    /// Remove a method from a registered service.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn remove_method(
        &self,
        registry_name: &str,
        service_name: &str,
        method: &str,
        method_type: &str,
    ) -> Result<()> {
        let path = format!("/registry/service/{registry_name}/methods");
        self.api
            .delete_with_params(
                ApiPath::templated(&path, "/registry/service/{registryName}/methods"),
                &[
                    ("serviceName", service_name),
                    ("method", method),
                    ("methodType", method_type),
                ],
            )
            .await
    }

    /// Get the raw contents of a `.proto` file stored under a service registry.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn get_proto_data(&self, registry_name: &str, filename: &str) -> Result<Vec<u8>> {
        let path = format!("/registry/service/protos/{registry_name}/{filename}");
        self.api
            .get_bytes(ApiPath::templated(
                &path,
                "/registry/service/protos/{registryName}/{filename}",
            ))
            .await
    }

    /// Upload (or overwrite) a `.proto` file under a service registry.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn set_proto_data(
        &self,
        registry_name: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<()> {
        let path = format!("/registry/service/protos/{registry_name}/{filename}");
        self.api
            .post_bytes_no_response(
                ApiPath::templated(&path, "/registry/service/protos/{registryName}/{filename}"),
                data,
            )
            .await
    }

    /// Delete a `.proto` file from a service registry.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, or an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status.
    pub async fn delete_proto(&self, registry_name: &str, filename: &str) -> Result<()> {
        let path = format!("/registry/service/protos/{registry_name}/{filename}");
        self.api
            .delete_no_content(ApiPath::templated(
                &path,
                "/registry/service/protos/{registryName}/{filename}",
            ))
            .await
    }

    /// List all `.proto` files stored under a service registry.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn get_all_protos(&self, registry_name: &str) -> Result<Vec<ProtoRegistryEntry>> {
        let path = format!("/registry/service/protos/{registry_name}");
        self.api
            .get(ApiPath::templated(
                &path,
                "/registry/service/protos/{registryName}",
            ))
            .await
    }

    /// Discover the methods exposed by a gRPC service via reflection.
    ///
    /// When `create` is `true` and no registry entry exists yet for `name`, the
    /// server creates one from the discovered methods.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::ConductorError::Http`] if the request fails at the transport level, an [`crate::error::ConductorError::Auth`]/[`crate::error::ConductorError::Api`]/[`crate::error::ConductorError::Server`] variant if the server responds with a non-2xx status, or [`crate::error::ConductorError::Json`] if the response body can't be deserialized.
    pub async fn discover(&self, name: &str, create: bool) -> Result<Vec<ServiceMethod>> {
        let path = format!("/registry/service/{name}/discover");
        let template = ApiPath::templated(&path, "/registry/service/{name}/discover");

        if create {
            self.api
                .get_with_params(template, &[("create", "true")])
                .await
        } else {
            self.api.get(template).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Configuration;

    #[test]
    fn test_service_registry_client_creation() {
        let config = Configuration::new("http://localhost:8080/api");
        let api = ApiClient::new(config).unwrap();
        let _client = ServiceRegistryClient::new(api);
    }
}
