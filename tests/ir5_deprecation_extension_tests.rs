//! IR-5 acceptance tests: plexus-macros captures deprecation metadata at
//! **activation**, **method**, and **parameter** scopes, and emits it onto
//! the generated `PluginSchema` / `MethodSchema` / `MethodSchema.params_meta`.
//!
//! Coverage map (acceptance criteria from `plans/IR/IR-5.md`):
//!
//! | AC | Verified by                                      |
//! |----|--------------------------------------------------|
//! | 3  | `activation_level_deprecation_populates_schema`  |
//! | 4  | `param_level_deprecation_on_specific_param`      |
//! | 6  | `tests/compile/ir5_hub_flag_warning.rs`          |
//! | 7  | `tests/compile/ir5_children_arg_warning.rs`      |
//!
//! ACs 1, 2 are checked by `cargo build -p plexus-core` and `cargo build -p
//! plexus-macros` respectively; AC 8 is checked by substrate's workspace
//! build.
//!
//! Run with: `cargo test --test ir5_deprecation_extension_tests`.

use async_stream::stream;
use futures::stream::Stream;
use plexus_core::plexus::{DeprecationInfo, MethodRole};

// ---------------------------------------------------------------------------
// AC #3: `#[deprecated(...)]` + `#[plexus_macros::removed_in(...)]` on the
// `impl Activation for Foo` block populates `PluginSchema.deprecation` with
// a fully-fleshed `DeprecationInfo`.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DeprecatedActivation;

#[deprecated(since = "0.5", note = "use NewActivation")]
#[plexus_macros::removed_in("0.7")]
#[plexus_macros::activation(
    namespace = "deprecated_activation",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl DeprecatedActivation {
    /// A plain method on a deprecated activation — not deprecated itself.
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }
}

#[test]
fn activation_level_deprecation_populates_schema() {
    use plexus_core::plexus::Activation;
    let activation = DeprecatedActivation;
    let schema = activation.plugin_schema();
    assert_eq!(
        schema.deprecation,
        Some(DeprecationInfo {
            since: "0.5".into(),
            removed_in: "0.7".into(),
            message: "use NewActivation".into(),
        }),
        "activation-level deprecation must be populated onto PluginSchema.deprecation"
    );
    // Individual methods on a deprecated activation are NOT implicitly
    // deprecated — their own MethodSchema.deprecation stays None.
    let methods = DeprecatedActivationMethod::method_schemas();
    let ping = methods.iter().find(|m| m.name == "ping").unwrap();
    assert!(
        ping.deprecation.is_none(),
        "method's own deprecation should be independent of activation-level \
         deprecation"
    );
    // Sanity: role is Rpc.
    assert_eq!(ping.role, MethodRole::Rpc);
}

// ---------------------------------------------------------------------------
// AC (pre-IR-5 default): activation WITHOUT any #[deprecated] emits
// PluginSchema.deprecation = None.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FreshActivation;

#[plexus_macros::activation(
    namespace = "fresh_activation",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl FreshActivation {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }
}

#[test]
fn undeprecated_activation_emits_none_deprecation() {
    use plexus_core::plexus::Activation;
    let activation = FreshActivation;
    let schema = activation.plugin_schema();
    assert!(
        schema.deprecation.is_none(),
        "fresh activation must emit PluginSchema.deprecation = None"
    );
}

// ---------------------------------------------------------------------------
// AC #4: `#[deprecated(...)]` on an individual parameter populates a
// `ParamSchema { name, deprecation }` entry on `MethodSchema.params_meta`
// for that param only — sibling parameters stay out of the list.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ParamDeprecationActivation;

#[plexus_macros::activation(
    namespace = "param_dep",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl ParamDeprecationActivation {
    /// Method with two params — only `b` is deprecated.
    #[plexus_macros::method]
    async fn take_two(
        &self,
        a: String,
        #[deprecated(since = "0.5", note = "renamed to new_param")] b: String,
    ) -> impl Stream<Item = String> + Send + 'static {
        let _ = (a, b);
        stream! { yield "ok".into(); }
    }
}

#[test]
fn param_level_deprecation_on_specific_param() {
    let schemas = ParamDeprecationActivationMethod::method_schemas();
    let take_two = schemas
        .iter()
        .find(|m| m.name == "take_two")
        .expect("take_two method present");

    // Exactly one params_meta entry — for `b`.
    assert_eq!(
        take_two.params_meta.len(),
        1,
        "only the deprecated param should appear in params_meta; got {:?}",
        take_two.params_meta
    );

    let b = &take_two.params_meta[0];
    assert_eq!(b.name, "b");
    assert_eq!(
        b.deprecation,
        Some(DeprecationInfo {
            since: "0.5".into(),
            removed_in: "unspecified".into(),
            message: "renamed to new_param".into(),
        }),
    );

    // Sibling param `a` has no entry → the search below returns None.
    assert!(
        !take_two.params_meta.iter().any(|p| p.name == "a"),
        "non-deprecated sibling parameter must NOT appear in params_meta"
    );
}

// ---------------------------------------------------------------------------
// Method-level deprecation (IR-3 behavior) still works and is independent
// of params_meta (IR-5) — sanity check that the two propagation paths
// don't interfere.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MixedDeprecationActivation;

#[plexus_macros::activation(
    namespace = "mixed_dep",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
#[allow(deprecated)]
impl MixedDeprecationActivation {
    /// Method is itself deprecated (IR-3) AND has a deprecated parameter (IR-5).
    #[deprecated(since = "0.5", note = "use new_m")]
    #[plexus_macros::removed_in("0.6")]
    #[plexus_macros::method]
    async fn old_m(
        &self,
        #[deprecated(since = "0.5", note = "renamed")] p: String,
    ) -> impl Stream<Item = String> + Send + 'static {
        let _ = p;
        stream! { yield "ok".into(); }
    }
}

#[test]
fn method_and_param_deprecation_coexist() {
    let schemas = MixedDeprecationActivationMethod::method_schemas();
    let m = schemas.iter().find(|m| m.name == "old_m").unwrap();

    // Method-level deprecation (IR-3 behavior preserved).
    assert_eq!(
        m.deprecation,
        Some(DeprecationInfo {
            since: "0.5".into(),
            removed_in: "0.6".into(),
            message: "use new_m".into(),
        }),
    );
    // Param-level deprecation (IR-5).
    assert_eq!(m.params_meta.len(), 1);
    assert_eq!(m.params_meta[0].name, "p");
    assert_eq!(
        m.params_meta[0].deprecation,
        Some(DeprecationInfo {
            since: "0.5".into(),
            removed_in: "unspecified".into(),
            message: "renamed".into(),
        }),
    );
}
