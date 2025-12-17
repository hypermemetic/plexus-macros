//! Tests for hub_method macro
//!
//! These tests verify that the macro generates valid code that compiles
//! and produces correct schema output.

use dialectic::types::{Choose, Continue, Done, Loop, Offer, Recv, Send};
use dialectic::Session;
use hub_macro::hub_method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use substrate::plexus::{MethodSchema, ProtocolSchema, SessionSchema};

// Test input/output types
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SimpleInput {
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SimpleOutput {
    result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct StreamChunk {
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct StreamComplete {
    total: i32,
}

// ============================================================================
// Test: Simple request-response
// ============================================================================

/// A simple method that takes input and returns output
#[hub_method(crate_path = "substrate")]
async fn simple_method(input: SimpleInput) -> Send<SimpleOutput, Done> {
    todo!()
}

#[test]
fn test_simple_method_generates_schema() {
    let schema = simple_method_schema();

    assert_eq!(schema.name, "simple_method");
    assert_eq!(
        schema.description,
        "A simple method that takes input and returns output"
    );

    // Client protocol: Send input, Recv output
    match &schema.protocol {
        ProtocolSchema::Send { then, .. } => match &**then {
            ProtocolSchema::Recv { then, .. } => {
                assert!(matches!(&**then, ProtocolSchema::Done));
            }
            _ => panic!("Expected Recv after Send in client protocol"),
        },
        _ => panic!("Expected Send first in client protocol"),
    }

    // Server protocol: Recv input, Send output
    match &schema.server_protocol {
        ProtocolSchema::Recv { then, .. } => match &**then {
            ProtocolSchema::Send { then, .. } => {
                assert!(matches!(&**then, ProtocolSchema::Done));
            }
            _ => panic!("Expected Send after Recv in server protocol"),
        },
        _ => panic!("Expected Recv first in server protocol"),
    }
}

// ============================================================================
// Test: Custom method name
// ============================================================================

/// Method with custom name
#[hub_method(name = "custom_name", crate_path = "substrate")]
async fn internal_name(input: SimpleInput) -> Send<SimpleOutput, Done> {
    todo!()
}

#[test]
fn test_custom_name() {
    let schema = internal_name_schema();
    assert_eq!(schema.name, "custom_name");
}

// ============================================================================
// Test: No input parameter
// ============================================================================

/// List all items
#[hub_method(crate_path = "substrate")]
async fn list_items() -> Send<SimpleOutput, Done> {
    todo!()
}

#[test]
fn test_no_input() {
    let schema = list_items_schema();

    assert_eq!(schema.name, "list_items");

    // Client protocol: Recv output (no send since no input)
    match &schema.protocol {
        ProtocolSchema::Recv { then, .. } => {
            assert!(matches!(&**then, ProtocolSchema::Done));
        }
        _ => panic!("Expected Recv first when no input"),
    }

    // Server protocol: Send output
    match &schema.server_protocol {
        ProtocolSchema::Send { then, .. } => {
            assert!(matches!(&**then, ProtocolSchema::Done));
        }
        _ => panic!("Expected Send first in server protocol when no input"),
    }
}

// ============================================================================
// Test: Streaming protocol with loop
// ============================================================================

/// Stream data with chunks
#[hub_method(crate_path = "substrate")]
async fn stream_data(
    input: SimpleInput,
) -> Loop<Choose<(Send<StreamChunk, Continue<0>>, Send<StreamComplete, Done>)>> {
    todo!()
}

#[test]
fn test_streaming_protocol() {
    let schema = stream_data_schema();

    assert_eq!(schema.name, "stream_data");

    // Server protocol should have: Recv input, Loop { Choose { ... } }
    match &schema.server_protocol {
        ProtocolSchema::Recv { then, .. } => match &**then {
            ProtocolSchema::Loop { body } => match &**body {
                ProtocolSchema::Choose { branches } => {
                    assert_eq!(branches.len(), 2);
                }
                _ => panic!("Expected Choose in loop body"),
            },
            _ => panic!("Expected Loop after Recv"),
        },
        _ => panic!("Expected Recv first"),
    }

    // Client protocol should have: Send input, Loop { Offer { ... } }
    match &schema.protocol {
        ProtocolSchema::Send { then, .. } => match &**then {
            ProtocolSchema::Loop { body } => match &**body {
                ProtocolSchema::Offer { branches } => {
                    assert_eq!(branches.len(), 2);
                }
                _ => panic!("Expected Offer in loop body (dual of Choose)"),
            },
            _ => panic!("Expected Loop after Send"),
        },
        _ => panic!("Expected Send first"),
    }
}

// ============================================================================
// Test: Schema contains JSON Schema for types
// ============================================================================

#[test]
fn test_schema_contains_type_info() {
    let schema = simple_method_schema();

    // Check client protocol has input type schema
    if let ProtocolSchema::Send { payload, .. } = &schema.protocol {
        // Should have properties from SimpleInput
        assert!(payload.get("properties").is_some() || payload.get("$schema").is_some());
    }
}

// ============================================================================
// Test: Multiline doc comment
// ============================================================================

/// This is a method with
/// multiple lines of documentation
/// that should be joined together
#[hub_method(crate_path = "substrate")]
async fn multiline_doc(input: SimpleInput) -> Send<SimpleOutput, Done> {
    todo!()
}

#[test]
fn test_multiline_doc() {
    let schema = multiline_doc_schema();
    assert!(schema.description.contains("multiple lines"));
    assert!(schema.description.contains("joined together"));
}

// ============================================================================
// Test: Schema serialization
// ============================================================================

#[test]
fn test_schema_serializes_to_json() {
    let schema = simple_method_schema();
    let json = serde_json::to_string_pretty(&schema).unwrap();

    assert!(json.contains("simple_method"));
    assert!(json.contains("protocol"));
    assert!(json.contains("server_protocol"));
}
