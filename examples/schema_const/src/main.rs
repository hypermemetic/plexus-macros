use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

// Goal: Get schema where "method" has { "const": "execute" } instead of { "type": "string" }

// Approach 1: Standard adjacently tagged enum
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BashMethod {
    Execute { command: String },
}

// Approach 2: Try using a const generic or literal type somehow
// This doesn't work directly with schemars

// Approach 3: Separate structs for each method with explicit schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteMethod {
    pub method: ExecuteMethodTag,
    pub params: ExecuteParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteMethodTag;

// Custom JsonSchema to emit const
impl JsonSchema for ExecuteMethodTag {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ExecuteMethodTag")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::from(schemars::json_schema!({
            "const": "execute"
        }))
    }
}

impl JsonSchema for ExecuteMethod {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ExecuteMethod")
    }

    fn json_schema(gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::Schema::from(schemars::json_schema!({
            "type": "object",
            "properties": {
                "method": gen.subschema_for::<ExecuteMethodTag>(),
                "params": gen.subschema_for::<ExecuteParams>()
            },
            "required": ["method", "params"]
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteParams {
    pub command: String,
}

// Approach 4: Use a wrapper that implements custom JsonSchema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BashMethodV2 {
    Execute { command: String },
}

impl JsonSchema for BashMethodV2 {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("BashMethodV2")
    }

    fn json_schema(gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Generate params schema
        #[derive(JsonSchema)]
        struct ExecuteParams {
            command: String,
        }

        let params_schema = gen.subschema_for::<ExecuteParams>();

        schemars::Schema::from(schemars::json_schema!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "method": { "const": "execute" },
                        "params": params_schema
                    },
                    "required": ["method", "params"]
                }
            ]
        }))
    }
}

// Approach 5: Multiple methods with custom schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MultiMethod {
    Execute { command: String },
    List,
}

impl JsonSchema for MultiMethod {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("MultiMethod")
    }

    fn json_schema(gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        #[derive(JsonSchema)]
        struct ExecuteParams {
            command: String,
        }

        let execute_params = gen.subschema_for::<ExecuteParams>();

        schemars::Schema::from(schemars::json_schema!({
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "method": { "const": "execute" },
                        "params": execute_params
                    },
                    "required": ["method", "params"]
                },
                {
                    "type": "object",
                    "properties": {
                        "method": { "const": "list" }
                    },
                    "required": ["method"]
                }
            ]
        }))
    }
}

fn main() {
    println!("=== Approach 1: Standard schemars (adjacently tagged) ===");
    let schema1 = schemars::schema_for!(BashMethod);
    println!("{}\n", serde_json::to_string_pretty(&schema1).unwrap());

    println!("=== Approach 3: Separate struct with custom tag schema ===");
    let schema3 = schemars::schema_for!(ExecuteMethod);
    println!("{}\n", serde_json::to_string_pretty(&schema3).unwrap());

    println!("=== Approach 4: Custom JsonSchema impl (single method) ===");
    let schema4 = schemars::schema_for!(BashMethodV2);
    println!("{}\n", serde_json::to_string_pretty(&schema4).unwrap());

    println!("=== Approach 5: Custom JsonSchema impl (multiple methods) ===");
    let schema5 = schemars::schema_for!(MultiMethod);
    println!("{}\n", serde_json::to_string_pretty(&schema5).unwrap());
}
