//! Tests for HandleEnum derive macro

use plexus_core::Handle;
use plexus_macros::HandleEnum;
use uuid::Uuid;

// Test plugin ID
const TEST_PLUGIN_ID: Uuid = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);

/// Test enum with resolution configuration
#[derive(Debug, Clone, HandleEnum)]
#[handle(plugin_id = "TEST_PLUGIN_ID", version = "1.0.0")]
pub enum TestHandle {
    /// Message handle with resolution
    #[handle(
        method = "chat",
        table = "messages",
        key = "id",
        key_field = "message_id",
        strip_prefix = "msg-"
    )]
    Message {
        message_id: String,
        role: String,
        name: String,
    },

    /// Event handle without resolution (passthrough)
    #[handle(method = "event")]
    Event { event_id: String, event_type: String },

    /// Unit variant
    #[handle(method = "ping")]
    Ping,
}

#[test]
fn test_to_handle_message() {
    let handle = TestHandle::Message {
        message_id: "msg-abc123".to_string(),
        role: "user".to_string(),
        name: "bob".to_string(),
    };

    let h = handle.to_handle();
    assert_eq!(h.plugin_id, TEST_PLUGIN_ID);
    assert_eq!(h.version, "1.0.0");
    assert_eq!(h.method, "chat");
    assert_eq!(h.meta, vec!["msg-abc123", "user", "bob"]);
}

#[test]
fn test_to_handle_event() {
    let handle = TestHandle::Event {
        event_id: "evt-123".to_string(),
        event_type: "click".to_string(),
    };

    let h = handle.to_handle();
    assert_eq!(h.method, "event");
    assert_eq!(h.meta, vec!["evt-123", "click"]);
}

#[test]
fn test_to_handle_unit() {
    let handle = TestHandle::Ping;
    let h = handle.to_handle();
    assert_eq!(h.method, "ping");
    assert!(h.meta.is_empty());
}

#[test]
fn test_try_from_handle_message() {
    let h = Handle::new(TEST_PLUGIN_ID, "1.0.0", "chat")
        .with_meta(vec!["msg-abc123".to_string(), "user".to_string(), "bob".to_string()]);

    let result = TestHandle::try_from(&h);
    assert!(result.is_ok());

    if let TestHandle::Message { message_id, role, name } = result.unwrap() {
        assert_eq!(message_id, "msg-abc123");
        assert_eq!(role, "user");
        assert_eq!(name, "bob");
    } else {
        panic!("Expected Message variant");
    }
}

#[test]
fn test_try_from_wrong_plugin() {
    let wrong_plugin = Uuid::from_u128(0x99999999_9999_9999_9999_999999999999);
    let h = Handle::new(wrong_plugin, "1.0.0", "chat")
        .with_meta(vec!["msg-abc123".to_string(), "user".to_string(), "bob".to_string()]);

    let result = TestHandle::try_from(&h);
    assert!(result.is_err());
}

#[test]
fn test_try_from_unknown_method() {
    let h = Handle::new(TEST_PLUGIN_ID, "1.0.0", "unknown_method")
        .with_meta(vec!["data".to_string()]);

    let result = TestHandle::try_from(&h);
    assert!(result.is_err());
}

#[test]
fn test_resolution_params_message() {
    let handle = TestHandle::Message {
        message_id: "msg-abc123".to_string(),
        role: "user".to_string(),
        name: "bob".to_string(),
    };

    let params = handle.resolution_params();
    assert!(params.is_some());

    let p = params.unwrap();
    assert_eq!(p.table, "messages");
    assert_eq!(p.key_column, "id");
    // strip_prefix should remove "msg-"
    assert_eq!(p.key_value, "abc123");
    // context should include non-key fields
    assert_eq!(p.context.len(), 2);
    assert!(p.context.iter().any(|(k, v)| k == "role" && v == "user"));
    assert!(p.context.iter().any(|(k, v)| k == "name" && v == "bob"));
}

#[test]
fn test_resolution_params_no_config() {
    let handle = TestHandle::Event {
        event_id: "evt-123".to_string(),
        event_type: "click".to_string(),
    };

    let params = handle.resolution_params();
    assert!(params.is_none());
}

#[test]
fn test_resolution_params_unit() {
    let handle = TestHandle::Ping;
    let params = handle.resolution_params();
    assert!(params.is_none());
}

#[test]
fn test_roundtrip() {
    let original = TestHandle::Message {
        message_id: "msg-xyz".to_string(),
        role: "assistant".to_string(),
        name: "claude".to_string(),
    };

    let h = original.to_handle();
    let restored = TestHandle::try_from(&h).unwrap();

    if let TestHandle::Message { message_id, role, name } = restored {
        assert_eq!(message_id, "msg-xyz");
        assert_eq!(role, "assistant");
        assert_eq!(name, "claude");
    } else {
        panic!("Expected Message variant");
    }
}
