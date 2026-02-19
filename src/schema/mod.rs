// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde_json::Value;

/// Generate a JSON Schema for a type
///
/// # Arguments
/// * `strict` - If true, sets `additionalProperties: false` to reject extra fields
///
/// # Example
/// ```rust
/// use conductor::schema::generate_schema;
/// use schemars::JsonSchema;
///
/// #[derive(JsonSchema)]
/// struct MyInput {
///     name: String,
///     value: i32,
/// }
///
/// let schema = generate_schema::<MyInput>(true);
/// println!("{}", serde_json::to_string_pretty(&schema).unwrap());
/// ```
pub fn generate_schema<T: JsonSchema>(strict: bool) -> Value {
    let schema = schema_for!(T);
    let mut value = serde_json::to_value(schema).unwrap_or(Value::Null);

    if strict {
        apply_strict_schema(&mut value);
    }

    value
}

/// Generate a JSON Schema with custom settings
pub fn generate_schema_with_settings<T: JsonSchema>(settings: &SchemaSettings) -> Value {
    let schema = schema_for!(T);
    let mut value = serde_json::to_value(schema).unwrap_or(Value::Null);

    if settings.strict {
        apply_strict_schema(&mut value);
    }

    value
}

/// Apply strict schema validation (additionalProperties: false) recursively
fn apply_strict_schema(value: &mut Value) {
    if let Value::Object(map) = value {
        // If this object has "properties", it's an object schema - add additionalProperties: false
        if map.contains_key("properties") {
            map.insert("additionalProperties".to_string(), Value::Bool(false));
        }

        // Recursively apply to nested schemas
        for (_, v) in map.iter_mut() {
            apply_strict_schema(v);
        }
    } else if let Value::Array(arr) = value {
        for item in arr.iter_mut() {
            apply_strict_schema(item);
        }
    }
}

/// Settings for schema generation
#[derive(Debug, Clone, Default)]
pub struct SchemaSettings {
    /// If true, sets additionalProperties: false
    pub strict: bool,
    /// Schema title override
    pub title: Option<String>,
    /// Schema description override
    pub description: Option<String>,
}

impl SchemaSettings {
    /// Create new schema settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable strict mode (additionalProperties: false)
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Set schema title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set schema description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Schema definition for registration with Conductor
#[derive(Debug, Clone)]
pub struct TaskSchema {
    /// Schema name (e.g., "task_name_input" or "task_name_output")
    pub name: String,
    /// JSON Schema content
    pub schema: Value,
    /// Schema version
    pub version: i32,
    /// Schema type (INPUT or OUTPUT)
    pub schema_type: SchemaType,
}

/// Type of schema
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaType {
    /// Input schema for task
    Input,
    /// Output schema for task
    Output,
}

impl TaskSchema {
    /// Create a new input schema
    pub fn input<T: JsonSchema>(task_name: &str, strict: bool) -> Self {
        Self {
            name: format!("{}_input", task_name),
            schema: generate_schema::<T>(strict),
            version: 1,
            schema_type: SchemaType::Input,
        }
    }

    /// Create a new output schema
    pub fn output<T: JsonSchema>(task_name: &str, strict: bool) -> Self {
        Self {
            name: format!("{}_output", task_name),
            schema: generate_schema::<T>(strict),
            version: 1,
            schema_type: SchemaType::Output,
        }
    }

    /// Set schema version
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = version;
        self
    }

    /// Convert to SchemaDef for registration
    pub fn to_schema_def(&self) -> crate::models::SchemaDef {
        crate::models::SchemaDef::new(self.name.clone(), self.version, self.schema.clone())
    }
}

/// Helper trait for workers that can generate schemas
pub trait WorkerSchema {
    /// Generate input schema for this worker
    fn input_schema(_strict: bool) -> Option<Value> {
        None
    }

    /// Generate output schema for this worker
    fn output_schema(_strict: bool) -> Option<Value> {
        None
    }
}

/// Get the root schema for a type
pub fn get_root_schema<T: JsonSchema>() -> RootSchema {
    schema_for!(T)
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestInput {
        name: String,
        value: i32,
        optional: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct TestOutput {
        result: String,
        count: i64,
    }

    #[test]
    fn test_generate_schema() {
        let schema = generate_schema::<TestInput>(false);

        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();

        // Should have properties
        assert!(obj.contains_key("properties"));

        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("value"));
        assert!(props.contains_key("optional"));
    }

    #[test]
    fn test_generate_strict_schema() {
        let schema = generate_schema::<TestInput>(true);

        let obj = schema.as_object().unwrap();

        // Should have additionalProperties: false
        assert_eq!(obj.get("additionalProperties"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_task_schema_input() {
        let schema = TaskSchema::input::<TestInput>("my_task", false);

        assert_eq!(schema.name, "my_task_input");
        assert_eq!(schema.schema_type, SchemaType::Input);
        assert_eq!(schema.version, 1);
    }

    #[test]
    fn test_task_schema_output() {
        let schema = TaskSchema::output::<TestOutput>("my_task", true);

        assert_eq!(schema.name, "my_task_output");
        assert_eq!(schema.schema_type, SchemaType::Output);

        // Should be strict
        let obj = schema.schema.as_object().unwrap();
        assert_eq!(obj.get("additionalProperties"), Some(&Value::Bool(false)));
    }

    #[test]
    fn test_to_schema_def() {
        let task_schema = TaskSchema::input::<TestInput>("process_order", false).with_version(2);

        let schema_def = task_schema.to_schema_def();

        assert_eq!(schema_def.name, "process_order_input");
        assert_eq!(schema_def.version, 2);
        assert_eq!(schema_def.schema_type, "JSON");
    }
}
