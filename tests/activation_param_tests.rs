//! REQ-2 acceptance tests: #[activation_param] compile-time and runtime behaviour.
//!
//! Compile-error tests live in tests/compile/ and are run via trybuild.
//! Run with: cargo test --test activation_param_tests

use trybuild::TestCases;

/// Tests that must compile successfully (positive cases).
#[test]
fn activation_param_compile_pass() {
    let t = TestCases::new();
    t.pass("tests/compile/activation_param_field_injection.rs");
    t.pass("tests/compile/from_auth_still_works.rs");
    t.pass("tests/compile/mixed_activation_param_and_from_auth.rs");
}

/// Tests that must produce specific compile errors (negative cases).
#[test]
fn activation_param_compile_fail() {
    let t = TestCases::new();
    t.compile_fail("tests/compile/activation_param_field_name_mismatch.rs");
    t.compile_fail("tests/compile/activation_param_type_mismatch.rs");
}

/// Tests that must compile successfully (REQ-4 activation-level request).
#[test]
fn activation_request_compile_pass() {
    let t = TestCases::new();
    t.pass("tests/compile/activation_request_type.rs");
    t.pass("tests/compile/activation_request_public_override.rs");
    t.pass("tests/compile/activation_no_request_unchanged.rs");
}

/// REQ-4 type mismatch with activation-level request.
#[test]
fn activation_request_compile_fail() {
    let t = TestCases::new();
    t.compile_fail("tests/compile/activation_request_field_type_mismatch.rs");
}
