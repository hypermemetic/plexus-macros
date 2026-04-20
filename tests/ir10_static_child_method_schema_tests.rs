#![allow(deprecated)]

//! IR-10 acceptance tests: static `#[child]` methods emit `MethodSchema`
//! entries with `role: StaticChild` unconditionally, regardless of whether
//! `plugin_children()` is hand-written or synthesized (CHILD-8 inferred hub).
//!
//! IR-3 suppressed those entries in the synthesized case to avoid tripping
//! plexus-core's pre-IR-4 `validate_no_collisions` panic. IR-4 relaxed that
//! check; IR-10 removes the suppression so downstream consumers (synapse,
//! hub-codegen, any IR reader) see the full role-tagged method list.
//!
//! Coverage map (acceptance criteria from `plans/IR/IR-10.md`):
//!
//! | AC | Verified by                                          |
//! |----|------------------------------------------------------|
//! | 3  | `static_children_emit_method_schema_with_handwritten_plugin_children` |
//! | 4  | `static_children_emit_method_schema_with_synthesized_plugin_children` |

use async_stream::stream;
use futures::stream::Stream;
use plexus_core::plexus::{Activation, MethodRole, MethodSchema};

// ---------------------------------------------------------------------------
// Shared leaf child used as the static-child target.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Leaf;

#[plexus_macros::activation(
    namespace = "leaf",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl Leaf {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }
}

fn find_method<'a>(methods: &'a [MethodSchema], name: &str) -> &'a MethodSchema {
    methods
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| {
            panic!(
                "method `{}` missing from methods list (saw: {:?})",
                name,
                methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// AC #3 companion: activation with `#[child]` static methods AND a
// hand-written `plugin_children()` override. Static-child `MethodSchema`
// entries were already present pre-IR-10 in this configuration (suppression
// only triggered when `plugin_children()` was synthesized). The test pins the
// behavior so future changes don't silently regress it.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct HandwrittenPluginChildrenHub {
    mercury: Leaf,
}

#[plexus_macros::activation(
    namespace = "handwritten_plugin_children_hub",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl HandwrittenPluginChildrenHub {
    /// Health check.
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// Static child — mercury.
    #[plexus_macros::child]
    fn mercury(&self) -> Leaf {
        self.mercury.clone()
    }

    /// Hand-written override suppresses CHILD-8 synthesis.
    pub fn plugin_children(&self) -> Vec<plexus_core::plexus::ChildSummary> {
        vec![plexus_core::plexus::ChildSummary {
            namespace: "mercury".into(),
            description: "hand-written".into(),
            hash: String::new(),
        }]
    }
}

#[test]
fn static_children_emit_method_schema_with_handwritten_plugin_children() {
    let hub = HandwrittenPluginChildrenHub { mercury: Leaf };
    let schema = hub.plugin_schema();

    let mercury = find_method(&schema.methods, "mercury");
    assert_eq!(
        mercury.role,
        MethodRole::StaticChild,
        "static #[child] method `mercury` must emit role = StaticChild"
    );

    // `ping` remains an Rpc method alongside.
    let ping = find_method(&schema.methods, "ping");
    assert_eq!(ping.role, MethodRole::Rpc);

    // Hand-written `plugin_children()` still drives `PluginSchema.children`.
    let children = schema.children.as_ref().expect("children populated");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].namespace, "mercury");
}

// ---------------------------------------------------------------------------
// AC #4: activation with `#[child]` static methods and NO hand-written
// `plugin_children()` override. The macro synthesizes `plugin_children()`
// (CHILD-8 inferred hub). Pre-IR-10 this case suppressed the
// `role: StaticChild` `MethodSchema` entries; post-IR-10 they must appear
// unconditionally.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SynthesizedPluginChildrenHub {
    mercury: Leaf,
    venus: Leaf,
}

#[plexus_macros::activation(
    namespace = "synthesized_plugin_children_hub",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl SynthesizedPluginChildrenHub {
    /// Health check.
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// Static child — mercury.
    #[plexus_macros::child]
    fn mercury(&self) -> Leaf {
        self.mercury.clone()
    }

    /// Static child — venus.
    #[plexus_macros::child]
    fn venus(&self) -> Leaf {
        self.venus.clone()
    }
}

#[test]
fn static_children_emit_method_schema_with_synthesized_plugin_children() {
    let hub = SynthesizedPluginChildrenHub {
        mercury: Leaf,
        venus: Leaf,
    };
    let schema = hub.plugin_schema();

    // IR-10: BOTH static children appear in the methods list with
    // `role: StaticChild`, even though plugin_children() was synthesized.
    let mercury = find_method(&schema.methods, "mercury");
    assert_eq!(mercury.role, MethodRole::StaticChild);

    let venus = find_method(&schema.methods, "venus");
    assert_eq!(venus.role, MethodRole::StaticChild);

    // `ping` still present with Rpc role.
    let ping = find_method(&schema.methods, "ping");
    assert_eq!(ping.role, MethodRole::Rpc);

    // Synthesized `plugin_children()` still drives `PluginSchema.children`:
    // the static-child method entries in `methods` and the `ChildSummary`
    // entries in `children` coexist (IR-4's `validate_no_collisions`
    // relaxation recognizes this overlap as construction-identical, not a
    // collision).
    let children = schema.children.as_ref().expect("children populated");
    let child_names: Vec<&str> =
        children.iter().map(|c| c.namespace.as_str()).collect();
    assert!(child_names.contains(&"mercury"));
    assert!(child_names.contains(&"venus"));
}
