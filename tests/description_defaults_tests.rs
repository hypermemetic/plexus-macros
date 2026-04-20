//! CHILD-5 acceptance tests: default `description` on
//! `#[plexus_macros::activation]` and `#[plexus_macros::method]` falls back to
//! `///` doc comments when no explicit `description = "..."` is supplied.
//!
//! Precedence (in order of priority, highest first):
//!   1. Explicit `description = "..."` on the attribute.
//!   2. `///` doc comments on the item (lines joined with `\n`, common
//!      leading whitespace stripped).
//!   3. Empty string.
//!
//! Run with: cargo test --test description_defaults_tests

use plexus_core::plexus::Activation;

// ---------------------------------------------------------------------------
// Case 1: single-line `///` doc comment, no explicit `description =`
// ---------------------------------------------------------------------------

struct SingleLineDocHub;

/// Foo
#[plexus_macros::activation(namespace = "single_line", version = "1.0.0", crate_path = "plexus_core")]
impl SingleLineDocHub {
    /// Foo
    #[plexus_macros::method]
    async fn do_thing(&self) -> impl futures::stream::Stream<Item = String> + Send + 'static {
        async_stream::stream! { yield "ok".into(); }
    }
}

#[test]
fn single_line_doc_comment_becomes_activation_description() {
    let hub = SingleLineDocHub;
    assert_eq!(hub.description(), "Foo");
    let schema = hub.plugin_schema();
    assert_eq!(schema.description, "Foo");
}

#[test]
fn single_line_doc_comment_becomes_method_description() {
    let schema = SingleLineDocHub.plugin_schema();
    let methods = &schema.methods;
    let do_thing = methods.iter().find(|m| m.name == "do_thing").expect("do_thing method");
    assert_eq!(do_thing.description, "Foo");
}

// ---------------------------------------------------------------------------
// Case 2: multi-line `///` doc comments preserve newlines
// ---------------------------------------------------------------------------

struct MultiLineDocHub;

/// Foo
/// Bar
#[plexus_macros::activation(namespace = "multi_line", version = "1.0.0", crate_path = "plexus_core")]
impl MultiLineDocHub {
    /// Foo
    /// Bar
    #[plexus_macros::method]
    async fn do_thing(&self) -> impl futures::stream::Stream<Item = String> + Send + 'static {
        async_stream::stream! { yield "ok".into(); }
    }
}

#[test]
fn multi_line_doc_comments_join_with_newlines_on_activation() {
    let hub = MultiLineDocHub;
    assert_eq!(hub.description(), "Foo\nBar");
}

#[test]
fn multi_line_doc_comments_join_with_newlines_on_method() {
    let schema = MultiLineDocHub.plugin_schema();
    let do_thing = schema.methods.iter().find(|m| m.name == "do_thing").expect("do_thing method");
    assert_eq!(do_thing.description, "Foo\nBar");
}

// ---------------------------------------------------------------------------
// Case 3: explicit `description = "..."` wins over a `///` doc comment
// ---------------------------------------------------------------------------

struct ExplicitWinsHub;

/// Foo
#[plexus_macros::activation(
    namespace = "explicit",
    version = "1.0.0",
    description = "Bar",
    crate_path = "plexus_core"
)]
impl ExplicitWinsHub {
    /// Foo
    #[plexus_macros::method(description = "Bar")]
    async fn do_thing(&self) -> impl futures::stream::Stream<Item = String> + Send + 'static {
        async_stream::stream! { yield "ok".into(); }
    }
}

#[test]
fn explicit_description_wins_over_doc_comment_on_activation() {
    let hub = ExplicitWinsHub;
    assert_eq!(hub.description(), "Bar");
}

#[test]
fn explicit_description_wins_over_doc_comment_on_method() {
    let schema = ExplicitWinsHub.plugin_schema();
    let do_thing = schema.methods.iter().find(|m| m.name == "do_thing").expect("do_thing method");
    assert_eq!(do_thing.description, "Bar");
}

// ---------------------------------------------------------------------------
// Case 4: neither doc comment nor explicit `description =` → empty string
// ---------------------------------------------------------------------------

struct EmptyHub;

#[plexus_macros::activation(namespace = "empty", version = "1.0.0", crate_path = "plexus_core")]
impl EmptyHub {
    #[plexus_macros::method]
    async fn do_thing(&self) -> impl futures::stream::Stream<Item = String> + Send + 'static {
        async_stream::stream! { yield "ok".into(); }
    }
}

#[test]
fn no_doc_and_no_explicit_description_is_empty_on_activation() {
    let hub = EmptyHub;
    assert_eq!(hub.description(), "");
}

#[test]
fn no_doc_and_no_explicit_description_is_empty_on_method() {
    let schema = EmptyHub.plugin_schema();
    let do_thing = schema.methods.iter().find(|m| m.name == "do_thing").expect("do_thing method");
    assert_eq!(do_thing.description, "");
}
