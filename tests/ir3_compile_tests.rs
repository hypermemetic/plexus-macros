//! IR-3 compile-fail and compile-pass fixtures for method-level deprecation
//! capture and the `#[plexus_macros::removed_in]` companion attribute.
//!
//! Positive fixtures must compile; negative fixtures must produce specific
//! compile errors. Expected stderr files live next to each negative fixture
//! and are generated / refreshed via `TRYBUILD=overwrite cargo test`.
//!
//! Run with: `cargo test --test ir3_compile_tests`.

use trybuild::TestCases;

#[test]
fn ir3_compile_pass() {
    let t = TestCases::new();
    // AC #4: `#[deprecated]` on an `#[activation]` impl block compiles cleanly.
    t.pass("tests/compile/ir3_activation_deprecated_compiles.rs");
}

#[test]
fn ir3_compile_fail() {
    let t = TestCases::new();
    // AC #6: `#[plexus_macros::removed_in]` without a companion `#[deprecated]`
    // attribute fails to compile with a message naming `#[deprecated]`.
    t.compile_fail("tests/compile/ir3_removed_in_without_deprecated.rs");
}
