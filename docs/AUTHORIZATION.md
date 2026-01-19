# Authorization API Reference

Complete API reference for authorization and RBAC operations in the Conductor Rust SDK.

> **Note**: Authorization APIs are available with Orkes Conductor.

## Table of Contents

- [Quick Start](#quick-start)
- [AuthorizationClient API](#authorizationclient-api)
- [Applications](#applications)
- [Users](#users)
- [Groups](#groups)
- [Permissions](#permissions)
- [Roles](#roles)
- [Access Keys](#access-keys)
- [Models Reference](#models-reference)
- [Best Practices](#best-practices)

---

## Quick Start

```rust
use conductor::{Configuration, ConductorClient};
use conductor::models::{
    CreateOrUpdateApplicationRequest, UpsertUserRequest, UpsertGroupRequest,
    SubjectRef, TargetRef, AccessType, SubjectType, TargetType,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Configuration::new("https://play.orkes.io/api")
        .with_auth("KEY_ID", "KEY_SECRET");
    
    let client = ConductorClient::new(config)?;
    let auth_client = client.authorization_client();

    // Create an application
    let app_request = CreateOrUpdateApplicationRequest::new("my-service");
    let app = auth_client.create_application(&app_request).await?;
    println!("Created application: {}", app.id);

    // Create a user
    let user_request = UpsertUserRequest::new("John Doe")
        .with_roles(vec!["USER".to_string()]);
    let user = auth_client.upsert_user(&user_request, "john@example.com").await?;
    println!("Created user: {}", user.id);

    // Grant permissions
    let subject = SubjectRef::new(SubjectType::User, "john@example.com");
    let target = TargetRef::new(TargetType::WorkflowDef, "my_workflow");
    auth_client.grant_permissions(&subject, &target, &[AccessType::Read, AccessType::Execute]).await?;

    Ok(())
}
```

---

## AuthorizationClient API

### Application APIs

| Method | Description |
|--------|-------------|
| `create_application()` | Create a new application |
| `get_application()` | Get application by ID |
| `list_applications()` | List all applications |
| `update_application()` | Update an application |
| `delete_application()` | Delete an application |
| `get_app_by_access_key_id()` | Get app ID from access key |

### User APIs

| Method | Description |
|--------|-------------|
| `upsert_user()` | Create or update a user |
| `get_user()` | Get user by ID |
| `list_users()` | List all users |
| `delete_user()` | Delete a user |
| `get_granted_permissions_for_user()` | Get user's permissions |
| `check_permissions()` | Check if user has permission |

### Group APIs

| Method | Description |
|--------|-------------|
| `upsert_group()` | Create or update a group |
| `get_group()` | Get group by ID |
| `list_groups()` | List all groups |
| `delete_group()` | Delete a group |
| `add_user_to_group()` | Add user to group |
| `add_users_to_group()` | Add multiple users |
| `get_users_in_group()` | Get users in group |
| `remove_user_from_group()` | Remove user from group |
| `remove_users_from_group()` | Remove multiple users |
| `get_granted_permissions_for_group()` | Get group's permissions |

### Permission APIs

| Method | Description |
|--------|-------------|
| `grant_permissions()` | Grant permissions to subject |
| `get_permissions()` | Get permissions for target |
| `remove_permissions()` | Remove permissions |

### Role APIs

| Method | Description |
|--------|-------------|
| `list_all_roles()` | List all roles |
| `list_system_roles()` | List system roles |
| `list_custom_roles()` | List custom roles |
| `list_available_permissions()` | List available permissions |
| `create_role()` | Create custom role |
| `get_role()` | Get role by name |
| `update_role()` | Update custom role |
| `delete_role()` | Delete custom role |

### Access Key APIs

| Method | Description |
|--------|-------------|
| `create_access_key()` | Create access key for app |
| `get_access_keys()` | Get app's access keys |
| `toggle_access_key_status()` | Enable/disable key |
| `delete_access_key()` | Delete access key |
| `add_role_to_application_user()` | Add role to app |
| `remove_role_from_application_user()` | Remove role from app |

---

## Applications

### Create Application

```rust
use conductor::models::CreateOrUpdateApplicationRequest;

let request = CreateOrUpdateApplicationRequest::new("payment-service");
let app = auth_client.create_application(&request).await?;

println!("Application ID: {}", app.id);
println!("Application Name: {}", app.name);
```

### Get Application

```rust
let app = auth_client.get_application("app-id-123").await?;
println!("Application: {} ({})", app.name, app.id);
```

### List Applications

```rust
let apps = auth_client.list_applications().await?;

for app in &apps {
    println!("{}: {}", app.id, app.name);
}
```

### Update Application

```rust
let request = CreateOrUpdateApplicationRequest::new("payment-service-v2");
let updated = auth_client.update_application(&request, "app-id-123").await?;
```

### Delete Application

```rust
auth_client.delete_application("app-id-123").await?;
```

### Application Tags

```rust
use conductor::models::MetadataTag;

// Set tags
let tags = vec![
    MetadataTag::new("environment", "production"),
    MetadataTag::new("team", "payments"),
];
auth_client.set_application_tags(&tags, "app-id-123").await?;

// Get tags
let tags = auth_client.get_application_tags("app-id-123").await?;

// Delete tags
auth_client.delete_application_tags(&[
    MetadataTag::new("team", "payments"),
], "app-id-123").await?;
```

---

## Users

### Create or Update User

```rust
use conductor::models::UpsertUserRequest;

let request = UpsertUserRequest::new("John Doe")
    .with_roles(vec!["USER".to_string(), "METADATA_MANAGER".to_string()]);

let user = auth_client.upsert_user(&request, "john@example.com").await?;
println!("User created: {}", user.id);
```

### Get User

```rust
let user = auth_client.get_user("john@example.com").await?;

println!("User: {}", user.name);
println!("Roles: {:?}", user.roles);
println!("Groups: {:?}", user.groups);
```

### List Users

```rust
// List human users only
let users = auth_client.list_users(false).await?;

// Include application users
let all_users = auth_client.list_users(true).await?;

for user in &users {
    println!("{}: {} ({:?})", user.id, user.name, user.roles);
}
```

### Delete User

```rust
auth_client.delete_user("john@example.com").await?;
```

### Get User Permissions

```rust
let permissions = auth_client.get_granted_permissions_for_user("john@example.com").await?;

for perm in &permissions {
    println!("Target: {:?}:{}", perm.target.target_type, perm.target.id);
    println!("Access: {:?}", perm.access);
}
```

### Check Permissions

```rust
let result = auth_client.check_permissions(
    "john@example.com",
    "WORKFLOW_DEF",
    "my_workflow",
).await?;

for (permission, granted) in &result {
    println!("{}: {}", permission, granted);
}
```

---

## Groups

### Create or Update Group

```rust
use conductor::models::UpsertGroupRequest;

let request = UpsertGroupRequest::new("Engineering Team")
    .with_roles(vec!["USER".to_string(), "WORKFLOW_MANAGER".to_string()]);

let group = auth_client.upsert_group(&request, "engineering-team").await?;
println!("Group created: {}", group.id);
```

### Get Group

```rust
let group = auth_client.get_group("engineering-team").await?;

println!("Group: {} ({})", group.id, group.description);
println!("Roles: {:?}", group.roles);
```

### List Groups

```rust
let groups = auth_client.list_groups().await?;

for group in &groups {
    println!("{}: {}", group.id, group.description);
}
```

### Manage Group Members

```rust
// Add single user
auth_client.add_user_to_group("engineering-team", "john@example.com").await?;

// Add multiple users
auth_client.add_users_to_group("engineering-team", &[
    "jane@example.com".to_string(),
    "bob@example.com".to_string(),
]).await?;

// Get users in group
let users = auth_client.get_users_in_group("engineering-team").await?;
for user in &users {
    println!("  {}: {}", user.id, user.name);
}

// Remove single user
auth_client.remove_user_from_group("engineering-team", "bob@example.com").await?;

// Remove multiple users
auth_client.remove_users_from_group("engineering-team", &[
    "john@example.com".to_string(),
]).await?;
```

### Delete Group

```rust
auth_client.delete_group("engineering-team").await?;
```

---

## Permissions

### Grant Permissions

```rust
use conductor::models::{SubjectRef, TargetRef, AccessType, SubjectType, TargetType};

// Grant to user
let subject = SubjectRef::new(SubjectType::User, "john@example.com");
let target = TargetRef::new(TargetType::WorkflowDef, "order_workflow");

auth_client.grant_permissions(
    &subject,
    &target,
    &[AccessType::Read, AccessType::Execute],
).await?;

// Grant to group
let group_subject = SubjectRef::new(SubjectType::Group, "engineering-team");
auth_client.grant_permissions(
    &group_subject,
    &target,
    &[AccessType::Read, AccessType::Execute, AccessType::Update],
).await?;
```

### Get Permissions

```rust
let target = TargetRef::new(TargetType::WorkflowDef, "order_workflow");
let permissions = auth_client.get_permissions(&target).await?;

for (access_type, subjects) in &permissions {
    println!("{}:", access_type);
    for subject in subjects {
        println!("  {:?}: {}", subject.subject_type, subject.id);
    }
}
```

### Remove Permissions

```rust
let subject = SubjectRef::new(SubjectType::User, "john@example.com");
let target = TargetRef::new(TargetType::WorkflowDef, "order_workflow");

auth_client.remove_permissions(
    &subject,
    &target,
    &[AccessType::Execute],  // Remove only execute permission
).await?;
```

---

## Roles

### List Roles

```rust
// All roles
let all_roles = auth_client.list_all_roles().await?;

// System roles only
let system_roles = auth_client.list_system_roles().await?;

// Custom roles only
let custom_roles = auth_client.list_custom_roles().await?;
```

### Create Custom Role

```rust
let role_def = serde_json::json!({
    "name": "workflow-operator",
    "description": "Can execute and monitor workflows",
    "permissions": [
        {
            "resource": "WORKFLOW_DEF",
            "actions": ["READ", "EXECUTE"]
        },
        {
            "resource": "WORKFLOW",
            "actions": ["READ", "EXECUTE"]
        }
    ]
});

let role = auth_client.create_role(&role_def).await?;
println!("Created role: {:?}", role);
```

### Get Role

```rust
let role = auth_client.get_role("workflow-operator").await?;
println!("Role: {:?}", role);
```

### Update Role

```rust
let update = serde_json::json!({
    "description": "Updated description",
    "permissions": [
        {
            "resource": "WORKFLOW_DEF",
            "actions": ["READ", "EXECUTE", "UPDATE"]
        }
    ]
});

let updated = auth_client.update_role("workflow-operator", &update).await?;
```

### Delete Role

```rust
auth_client.delete_role("workflow-operator").await?;
```

---

## Access Keys

### Create Access Key

```rust
let access_key = auth_client.create_access_key("app-id-123").await?;

// IMPORTANT: Save these immediately - secret is only shown once!
println!("Key ID: {}", access_key.id);
println!("Key Secret: {}", access_key.secret);
```

### Get Access Keys

```rust
let keys = auth_client.get_access_keys("app-id-123").await?;

for key in &keys {
    println!("Key: {} - {:?}", key.id, key.status);
}
```

### Toggle Key Status

```rust
let key = auth_client.toggle_access_key_status("app-id-123", "key-id").await?;
println!("New status: {:?}", key.status);
```

### Delete Access Key

```rust
auth_client.delete_access_key("app-id-123", "key-id").await?;
```

### Application Roles

```rust
// Add role to application
auth_client.add_role_to_application_user("app-id-123", "ADMIN").await?;

// Remove role
auth_client.remove_role_from_application_user("app-id-123", "ADMIN").await?;
```

---

## Models Reference

### SubjectRef

```rust
pub struct SubjectRef {
    pub subject_type: SubjectType,
    pub id: String,
}

pub enum SubjectType {
    User,
    Group,
    Role,
}
```

### TargetRef

```rust
pub struct TargetRef {
    pub target_type: TargetType,
    pub id: String,
}

pub enum TargetType {
    WorkflowDef,
    TaskDef,
    Application,
    User,
    SecretName,
    Domain,
    // ... more types
}
```

### AccessType

```rust
pub enum AccessType {
    Read,
    Create,
    Update,
    Execute,
    Delete,
}
```

### System Roles

| Role | Description |
|------|-------------|
| `USER` | Basic user access |
| `ADMIN` | Full administrative access |
| `METADATA_MANAGER` | Manage workflow/task definitions |
| `WORKFLOW_MANAGER` | Manage workflow executions |
| `WORKER` | Worker task execution access |

---

## Best Practices

### 1. Use Groups for Permissions

```rust
// Instead of granting to individual users
// Grant to groups and add users to groups

let group_request = UpsertGroupRequest::new("Workflow Operators")
    .with_roles(vec!["USER".to_string()]);
auth_client.upsert_group(&group_request, "workflow-operators").await?;

// Grant permissions to group
let subject = SubjectRef::new(SubjectType::Group, "workflow-operators");
let target = TargetRef::new(TargetType::WorkflowDef, "order_workflow");
auth_client.grant_permissions(&subject, &target, &[AccessType::Read, AccessType::Execute]).await?;

// Add users to group
auth_client.add_users_to_group("workflow-operators", &[
    "user1@example.com".to_string(),
    "user2@example.com".to_string(),
]).await?;
```

### 2. Secure Access Keys

```rust
// Create access key and store securely
let key = auth_client.create_access_key(&app.id).await?;

// Store in secure vault/secrets manager
store_in_vault("conductor_key_id", &key.id);
store_in_vault("conductor_key_secret", &key.secret);

// Rotate keys periodically
async fn rotate_key(client: &AuthorizationClient, app_id: &str) -> Result<()> {
    // Create new key
    let new_key = client.create_access_key(app_id).await?;
    
    // Update application to use new key
    // ...
    
    // Delete old key
    let old_keys = client.get_access_keys(app_id).await?;
    for key in old_keys.iter().filter(|k| k.id != new_key.id) {
        client.delete_access_key(app_id, &key.id).await?;
    }
    
    Ok(())
}
```

### 3. Audit Permissions

```rust
async fn audit_workflow_access(client: &AuthorizationClient, workflow_name: &str) -> Result<()> {
    let target = TargetRef::new(TargetType::WorkflowDef, workflow_name);
    let permissions = client.get_permissions(&target).await?;
    
    println!("Access audit for workflow: {}", workflow_name);
    println!("==========================================");
    
    for (access_type, subjects) in &permissions {
        println!("\n{} access:", access_type);
        for subject in subjects {
            match subject.subject_type {
                SubjectType::User => {
                    let user = client.get_user(&subject.id).await?;
                    println!("  User: {} ({})", user.name, subject.id);
                }
                SubjectType::Group => {
                    let group = client.get_group(&subject.id).await?;
                    println!("  Group: {} ({} members)", subject.id, 
                        client.get_users_in_group(&subject.id).await?.len());
                }
                _ => println!("  {:?}: {}", subject.subject_type, subject.id),
            }
        }
    }
    
    Ok(())
}
```

---

## See Also

- [Secret Management](./SECRET_MANAGEMENT.md) - Managing secrets
- [Workflow Management](./WORKFLOW.md) - Workflows and permissions
- [Metadata Management](./METADATA.md) - Task/Workflow definitions
