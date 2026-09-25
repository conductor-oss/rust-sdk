// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Conductor user
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductorUser {
    /// User ID
    pub id: String,

    /// User name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// User email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Roles assigned to the user
    #[serde(default)]
    pub roles: Vec<Role>,

    /// Groups the user belongs to
    #[serde(default)]
    pub groups: Vec<Group>,

    /// Is application user
    #[serde(default)]
    pub application_user: bool,
}

/// Upsert user request
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertUserRequest {
    /// User name
    pub name: String,

    /// User email (optional for app users)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Roles to assign
    #[serde(default)]
    pub roles: Vec<String>,

    /// Groups to add user to
    #[serde(default)]
    pub groups: Vec<String>,
}

impl UpsertUserRequest {
    /// Create a new upsert user request
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set email
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Add roles
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// Add groups
    pub fn with_groups(mut self, groups: Vec<String>) -> Self {
        self.groups = groups;
        self
    }
}

/// User group
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Group ID
    pub id: String,

    /// Group name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Group description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Roles assigned to the group
    #[serde(default)]
    pub roles: Vec<Role>,

    /// Default access for group
    #[serde(default)]
    pub default_access: HashMap<String, Vec<String>>,
}

/// Upsert group request
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertGroupRequest {
    /// Group name
    pub name: String,

    /// Group description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Roles to assign
    #[serde(default)]
    pub roles: Vec<String>,

    /// Default access permissions
    #[serde(default)]
    pub default_access: HashMap<String, Vec<String>>,
}

impl UpsertGroupRequest {
    /// Create a new upsert group request
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add roles
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }
}

/// Role definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    /// Role name
    pub name: String,

    /// Role permissions
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

/// Permission
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    /// Permission name
    pub name: String,
}

/// Conductor application
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConductorApplication {
    /// Application ID
    pub id: String,

    /// Application name
    pub name: String,

    /// Create time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Update time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,

    /// Updated by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Create or update application request
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrUpdateApplicationRequest {
    /// Application name
    pub name: String,
}

impl CreateOrUpdateApplicationRequest {
    /// Create a new request
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Access key for application
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessKey {
    /// Key ID
    pub id: String,

    /// Key status
    pub status: String,

    /// Create time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,

    /// Created by
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Created access key (includes secret, only returned on creation)
///
/// Note: unlike `AccessKey`, the server's create-access-key response
/// (`POST /applications/{id}/accessKeys`) never includes a `status` field --
/// it's only present on the list/toggle responses (`AccessKey`) -- so it's
/// intentionally omitted here rather than left as a required field that
/// would fail to deserialize.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedAccessKey {
    /// Key ID
    pub id: String,

    /// Key secret (only available on creation)
    pub secret: String,
}

/// Subject reference (user or group)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectRef {
    /// Subject type (USER or GROUP)
    #[serde(rename = "type")]
    pub subject_type: SubjectType,

    /// Subject ID
    pub id: String,
}

impl SubjectRef {
    /// Create a user subject reference
    pub fn user(id: impl Into<String>) -> Self {
        Self {
            subject_type: SubjectType::User,
            id: id.into(),
        }
    }

    /// Create a group subject reference
    pub fn group(id: impl Into<String>) -> Self {
        Self {
            subject_type: SubjectType::Group,
            id: id.into(),
        }
    }
}

/// Subject type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubjectType {
    User,
    Group,
}

/// Target reference (resource being accessed)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    /// Target type
    #[serde(rename = "type")]
    pub target_type: TargetType,

    /// Target ID
    pub id: String,
}

impl TargetRef {
    /// Create a workflow definition target
    pub fn workflow(name: impl Into<String>) -> Self {
        Self {
            target_type: TargetType::WorkflowDef,
            id: name.into(),
        }
    }

    /// Create a task definition target
    pub fn task(name: impl Into<String>) -> Self {
        Self {
            target_type: TargetType::TaskDef,
            id: name.into(),
        }
    }

    /// Create a secret target
    pub fn secret(name: impl Into<String>) -> Self {
        Self {
            target_type: TargetType::Secret,
            id: name.into(),
        }
    }

    /// Create a domain target
    pub fn domain(name: impl Into<String>) -> Self {
        Self {
            target_type: TargetType::Domain,
            id: name.into(),
        }
    }
}

/// Target type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetType {
    WorkflowDef,
    TaskDef,
    Secret,
    Domain,
    Tag,
    Integration,
}

/// Access type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccessType {
    Create,
    Read,
    Update,
    Delete,
    Execute,
}

/// Granted permission
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantedPermission {
    /// Target
    pub target: Option<TargetRef>,

    /// Access types granted
    #[serde(default)]
    pub access: Vec<AccessType>,

    /// Tag the grant was made through, when the grant came from a tag rather
    /// than a direct target (server-side `GrantedAccess.tag`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}
