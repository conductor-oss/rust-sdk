// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A named HTTP or gRPC parameter accepted by a [`ServiceMethod`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestParam {
    /// Parameter name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Parameter type (e.g. `"string"`, `"int"`).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub param_type: Option<String>,

    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One invocable method (REST operation or gRPC unary/streaming call) exposed
/// by a registered service.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMethod {
    /// Server-assigned method id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,

    /// Name of the operation as exposed to workflows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,

    /// Underlying method name (REST path or gRPC rpc name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_name: Option<String>,

    /// Method kind: `GET`, `PUT`, `POST`, `UNARY`, `SERVER_STREAMING`, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_type: Option<String>,

    /// Fully-qualified input type name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,

    /// Fully-qualified output type name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,

    /// Parameters accepted by the method.
    #[serde(default)]
    pub request_params: Vec<RequestParam>,

    /// Example input payload, for documentation/testing.
    #[serde(default)]
    pub example_input: Value,
}

/// Circuit breaker tuning knobs for a registered service.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_rate_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sliding_window_size: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_number_of_calls: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_duration_in_open_state: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub permitted_number_of_calls_in_half_open_state: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_call_rate_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_call_duration_threshold: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic_transition_from_open_to_half_open_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wait_duration_in_half_open_state: Option<i64>,
}

/// Per-service configuration, currently just the circuit breaker settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRegistryConfig {
    #[serde(default)]
    pub circuit_breaker_config: CircuitBreakerConfig,
}

/// A registered HTTP or gRPC service that workflows can call through
/// [`crate::client::ServiceRegistryClient`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRegistry {
    /// Service name, used as the registry key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Service kind: `"HTTP"` or `"gRPC"`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,

    /// Base URI the service is reachable at.
    #[serde(rename = "serviceURI", skip_serializing_if = "Option::is_none")]
    pub service_uri: Option<String>,

    /// Methods exposed by this service.
    #[serde(default)]
    pub methods: Vec<ServiceMethod>,

    /// Request parameters shared across the service's methods.
    #[serde(default)]
    pub request_params: Vec<RequestParam>,

    /// Circuit breaker configuration.
    #[serde(default)]
    pub config: ServiceRegistryConfig,
}

impl ServiceRegistry {
    /// Create a new HTTP service registration.
    #[must_use]
    pub fn http(name: impl Into<String>, service_uri: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            service_type: Some("HTTP".to_owned()),
            service_uri: Some(service_uri.into()),
            ..Self::default()
        }
    }

    /// Create a new gRPC service registration.
    #[must_use]
    pub fn grpc(name: impl Into<String>, service_uri: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            service_type: Some("gRPC".to_owned()),
            service_uri: Some(service_uri.into()),
            ..Self::default()
        }
    }
}

/// One `.proto` file stored alongside a gRPC [`ServiceRegistry`] entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtoRegistryEntry {
    /// Name of the service registry the proto belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,

    /// Proto file name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// Result of an `open`/`close` transition, or the current status, of a
/// service's circuit breaker.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerTransitionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_state: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_timestamp: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CircuitBreakerTransitionResponse {
    /// Whether the circuit breaker is currently open (tripped).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.current_state
            .as_deref()
            .is_some_and(|state| state.eq_ignore_ascii_case("open"))
    }
}
