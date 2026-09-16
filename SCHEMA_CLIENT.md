# Schema Client

`SchemaClient` manages Conductor **schema definitions** — reusable JSON Schema documents
(`SchemaDef`) that can be attached to workflow/task inputs and outputs for validation, separately
from a task worker's own `#[worker]`-generated schema. This is already fully implemented
(`src/client/schema_client.rs`); this file just documents it, since it had no standalone doc yet.

## Getting a client

```rust
use conductor::client::ConductorClient;
use conductor::configuration::Configuration;

let config = Configuration::new("http://localhost:8080/api");
let client = ConductorClient::new(config)?;
let schema_client = client.schema_client();
// or, matching python-sdk naming: client.get_schema_client()
```

## `SchemaDef`

```rust
pub struct SchemaDef {
    pub name: String,
    pub version: i32,
    pub schema_type: String,       // wire key "type", defaults to "JSON"
    pub data: serde_json::Value,   // the actual JSON Schema document
    pub external_ref: bool,        // whether this schema is used for external storage
    pub create_time: Option<i64>,
    pub created_by: Option<String>,
    pub update_time: Option<i64>,
    pub updated_by: Option<String>,
}
```

`SchemaDef::new(name, version, data)` fills in `schema_type` (`"JSON"`) and leaves the
server-populated audit fields (`create_time`/`created_by`/`update_time`/`updated_by`) unset —
the server fills those in on register and returns them on read.

## Operations

| Method | Endpoint | Notes |
|---|---|---|
| `register_schema(&SchemaDef)` | `POST /schema` | Create or update a schema. |
| `get_schema(name, version)` | `GET /schema/{name}?version={version}` | Fetch one version. |
| `get_all_schemas()` | `GET /schema` | List every registered schema (all names/versions). |
| `delete_schema(name, version)` | `DELETE /schema/{name}?version={version}` | Delete one version. |
| `delete_schema_by_name(name)` | `DELETE /schema/{name}/all` | Delete every version of a schema. |

## Example

```rust
use conductor::models::SchemaDef;
use serde_json::json;

let schema = SchemaDef::new(
    "order-input",
    1,
    json!({
        "type": "object",
        "properties": {
            "order_id": {"type": "string"},
            "amount": {"type": "number"}
        },
        "required": ["order_id", "amount"]
    }),
);

schema_client.register_schema(&schema).await?;

let fetched = schema_client.get_schema("order-input", 1).await?;
assert_eq!(fetched.name, "order-input");

let all = schema_client.get_all_schemas().await?;

schema_client.delete_schema("order-input", 1).await?;
// or, to remove every version at once:
schema_client.delete_schema_by_name("order-input").await?;
```

## Errors

All methods return `Result<T, ConductorError>`:

- `ConductorError::Http` — the request failed at the transport level.
- `ConductorError::Auth` / `ConductorError::Api` / `ConductorError::Server` — the server
  responded with a non-2xx status (401, 400/404, or another non-2xx status respectively).
- `ConductorError::Json` — the response body couldn't be deserialized (read operations only).

## Related Documentation

- **[src/client/schema_client.rs](src/client/schema_client.rs)** — implementation
- **[src/models/schema.rs](src/models/schema.rs)** — `SchemaDef` model
