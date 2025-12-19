//! Schema Query Example
//!
//! This demonstrates:
//! 1. A simple plugin with streaming and non-streaming methods
//! 2. Registering the plugin with a hub
//! 3. Querying the hub for the full schema at runtime

use futures::{Stream, StreamExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

// ============================================================================
// Part 1: Schema Types (what the hub exposes)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HubSchema {
    pub activations: HashMap<String, ActivationSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActivationSchema {
    pub name: String,
    pub description: Option<String>,
    pub methods: HashMap<String, MethodSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MethodSchema {
    pub name: String,
    pub description: Option<String>,
    pub params: serde_json::Value,
    pub returns: ReturnSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReturnSchema {
    Value {
        schema: serde_json::Value,
    },
    Stream {
        item_schema: serde_json::Value,
        terminal_variants: Vec<String>,
    },
}

// ============================================================================
// Part 2: StreamEvent Trait (what #[derive(StreamEvent)] would generate)
// ============================================================================

pub trait StreamEvent: Serialize + JsonSchema {
    fn is_terminal(&self) -> bool;
    fn terminal_variants() -> Vec<&'static str>;
}

// ============================================================================
// Part 3: Activation Trait (what plugins implement)
// ============================================================================

pub trait Activation: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn schema(&self) -> ActivationSchema;
}

// ============================================================================
// Part 4: A Simple Plugin - Echo
// ============================================================================

/// Events emitted by the echo stream
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EchoEvent {
    /// An echoed message chunk
    Chunk { data: String },
    /// Stream complete
    Done { total_chars: usize },
}

// This is what #[derive(StreamEvent)] would generate
impl StreamEvent for EchoEvent {
    fn is_terminal(&self) -> bool {
        matches!(self, EchoEvent::Done { .. })
    }

    fn terminal_variants() -> Vec<&'static str> {
        vec!["done"]
    }
}

/// Status response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EchoStatus {
    pub uptime_secs: u64,
    pub messages_processed: u64,
}

/// The Echo plugin
pub struct EchoActivation {
    start_time: std::time::Instant,
    message_count: std::sync::atomic::AtomicU64,
}

impl EchoActivation {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            message_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    // --- Method implementations (what the user writes) ---

    /// Echo back the input as a stream of chunks
    pub fn echo(&self, message: String) -> impl Stream<Item = EchoEvent> {
        self.message_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let total = message.len();

        async_stream::stream! {
            // Stream each word as a chunk
            for word in message.split_whitespace() {
                yield EchoEvent::Chunk { data: word.to_string() };
            }
            // Terminal event
            yield EchoEvent::Done { total_chars: total };
        }
    }

    /// Get current status (non-streaming)
    pub fn status(&self) -> EchoStatus {
        EchoStatus {
            uptime_secs: self.start_time.elapsed().as_secs(),
            messages_processed: self
                .message_count
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

// --- What #[hub_methods] would generate ---

impl Activation for EchoActivation {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn schema(&self) -> ActivationSchema {
        let mut methods = HashMap::new();

        // echo method schema
        methods.insert(
            "echo".to_string(),
            MethodSchema {
                name: "echo".to_string(),
                description: Some("Echo back the input as a stream of chunks".to_string()),
                params: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"]
                }),
                returns: ReturnSchema::Stream {
                    item_schema: serde_json::to_value(schemars::schema_for!(EchoEvent)).unwrap(),
                    terminal_variants: EchoEvent::terminal_variants()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                },
            },
        );

        // status method schema
        methods.insert(
            "status".to_string(),
            MethodSchema {
                name: "status".to_string(),
                description: Some("Get current status".to_string()),
                params: serde_json::json!({ "type": "object", "properties": {} }),
                returns: ReturnSchema::Value {
                    schema: serde_json::to_value(schemars::schema_for!(EchoStatus)).unwrap(),
                },
            },
        );

        ActivationSchema {
            name: "echo".to_string(),
            description: Some("A simple echo plugin".to_string()),
            methods,
        }
    }
}

// ============================================================================
// Part 5: The Hub (holds activations, exposes schema)
// ============================================================================

pub struct Hub {
    activations: HashMap<String, Box<dyn Activation>>,
}

impl Hub {
    pub fn new() -> Self {
        Self {
            activations: HashMap::new(),
        }
    }

    pub fn register<A: Activation>(&mut self, activation: A) {
        let name = activation.name().to_string();
        self.activations.insert(name, Box::new(activation));
    }

    /// Query the full hub schema
    pub fn schema(&self) -> HubSchema {
        let mut activations = HashMap::new();
        for (name, activation) in &self.activations {
            activations.insert(name.clone(), activation.schema());
        }
        HubSchema { activations }
    }
}

// ============================================================================
// Part 6: Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_schema_query() {
        // Create hub
        let mut hub = Hub::new();

        // Register echo plugin
        hub.register(EchoActivation::new());

        // Query schema
        let schema = hub.schema();

        // Verify structure
        assert!(schema.activations.contains_key("echo"));

        let echo_schema = &schema.activations["echo"];
        assert_eq!(echo_schema.name, "echo");
        assert!(echo_schema.methods.contains_key("echo"));
        assert!(echo_schema.methods.contains_key("status"));

        // Check echo method is streaming
        let echo_method = &echo_schema.methods["echo"];
        match &echo_method.returns {
            ReturnSchema::Stream {
                terminal_variants, ..
            } => {
                assert!(terminal_variants.contains(&"done".to_string()));
            }
            _ => panic!("echo should be a streaming method"),
        }

        // Check status method is non-streaming
        let status_method = &echo_schema.methods["status"];
        assert!(matches!(status_method.returns, ReturnSchema::Value { .. }));

        // Print the full schema
        println!(
            "Hub Schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }

    #[test]
    fn test_schema_contains_json_schema_for_types() {
        let mut hub = Hub::new();
        hub.register(EchoActivation::new());

        let schema = hub.schema();
        let echo_method = &schema.activations["echo"].methods["echo"];

        match &echo_method.returns {
            ReturnSchema::Stream { item_schema, .. } => {
                let schema_str = serde_json::to_string(item_schema).unwrap();
                // Should contain the EchoEvent variants
                assert!(schema_str.contains("chunk"), "Should have chunk variant");
                assert!(schema_str.contains("done"), "Should have done variant");
                assert!(schema_str.contains("data"), "Should have data field");
                assert!(
                    schema_str.contains("total_chars"),
                    "Should have total_chars field"
                );
            }
            _ => panic!("Expected stream"),
        }
    }

    #[tokio::test]
    async fn test_plugin_actually_works() {
        let plugin = EchoActivation::new();

        // Call the streaming method
        let stream = plugin.echo("hello world test".to_string());
        tokio::pin!(stream);

        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should have 3 chunks + 1 done
        assert_eq!(events.len(), 4);
        assert!(matches!(&events[0], EchoEvent::Chunk { data } if data == "hello"));
        assert!(matches!(&events[1], EchoEvent::Chunk { data } if data == "world"));
        assert!(matches!(&events[2], EchoEvent::Chunk { data } if data == "test"));
        assert!(matches!(&events[3], EchoEvent::Done { total_chars: 16 }));

        // Check terminal detection
        assert!(!events[0].is_terminal());
        assert!(!events[1].is_terminal());
        assert!(!events[2].is_terminal());
        assert!(events[3].is_terminal());
    }

    #[test]
    fn test_multiple_activations() {
        // Create a second simple activation
        struct PingActivation;

        #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
        pub struct PongResponse {
            pub message: String,
        }

        impl Activation for PingActivation {
            fn name(&self) -> &'static str {
                "ping"
            }

            fn schema(&self) -> ActivationSchema {
                let mut methods = HashMap::new();
                methods.insert(
                    "ping".to_string(),
                    MethodSchema {
                        name: "ping".to_string(),
                        description: Some("Ping pong".to_string()),
                        params: serde_json::json!({ "type": "object", "properties": {} }),
                        returns: ReturnSchema::Value {
                            schema: serde_json::to_value(schemars::schema_for!(PongResponse))
                                .unwrap(),
                        },
                    },
                );

                ActivationSchema {
                    name: "ping".to_string(),
                    description: Some("Ping pong service".to_string()),
                    methods,
                }
            }
        }

        // Register multiple activations
        let mut hub = Hub::new();
        hub.register(EchoActivation::new());
        hub.register(PingActivation);

        let schema = hub.schema();

        // Both should be present
        assert_eq!(schema.activations.len(), 2);
        assert!(schema.activations.contains_key("echo"));
        assert!(schema.activations.contains_key("ping"));

        println!(
            "Multi-activation schema:\n{}",
            serde_json::to_string_pretty(&schema).unwrap()
        );
    }
}

fn main() {
    // Demo: create hub, register plugin, query schema
    let mut hub = Hub::new();
    hub.register(EchoActivation::new());

    let schema = hub.schema();
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
