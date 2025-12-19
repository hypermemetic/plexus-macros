use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Exactly like BashMethod is generated
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BashMethod {
    #[serde(rename = "execute")]
    Execute { command: String },
}

fn main() {
    let schema = schemars::schema_for!(BashMethod);
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
