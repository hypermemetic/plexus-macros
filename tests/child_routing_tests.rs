//! CHILD-3 acceptance tests: runtime dispatch for the
//! `#[plexus_macros::child]` method attribute.
//!
//! Covers acceptance criteria 2 and 3 of CHILD-3:
//!
//! - Static-only activation: `get_child("mercury")` → `Some(_)`,
//!   `get_child("unknown")` → `None`.
//! - Static + dynamic activation: `get_child("mercury")` resolves via the
//!   static arm; `get_child("planet_x")` resolves via the dynamic arm;
//!   `get_child("reject_me")` returns `None` when the dynamic arm declines.
//!
//! Compile-error tests live in `tests/compile/` and are run via trybuild
//! from `tests/child_compile_tests.rs`.

use async_stream::stream;
use futures::stream::Stream;
use plexus_core::plexus::{ChildCapabilities, ChildRouter};

// ---------------------------------------------------------------------------
// Shared leaf child used by both the static and dynamic test activations.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Mercury {
    /// Tag so tests can assert which child was returned.
    tag: &'static str,
}

#[plexus_macros::activation(
    namespace = "mercury",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl Mercury {
    /// Return the leaf tag.
    #[plexus_macros::method]
    async fn info(&self) -> impl Stream<Item = String> + Send + 'static {
        let tag = self.tag;
        stream! { yield tag.to_string(); }
    }
}

#[derive(Clone)]
struct Venus {
    tag: &'static str,
}

#[plexus_macros::activation(
    namespace = "venus",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl Venus {
    /// Return the leaf tag.
    #[plexus_macros::method]
    async fn info(&self) -> impl Stream<Item = String> + Send + 'static {
        let tag = self.tag;
        stream! { yield tag.to_string(); }
    }
}

// Dynamic children re-use Mercury as the concrete leaf type.

// ---------------------------------------------------------------------------
// Case 1: static-only #[child] methods.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct StaticOnlySolar {
    mercury: Mercury,
    venus: Venus,
}

#[plexus_macros::activation(
    namespace = "static_solar",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl StaticOnlySolar {
    /// Health check — keeps the activation from being method-empty.
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// The Mercury child.
    #[plexus_macros::child]
    fn mercury(&self) -> Mercury {
        self.mercury.clone()
    }

    /// The Venus child.
    #[plexus_macros::child]
    fn venus(&self) -> Venus {
        self.venus.clone()
    }
}

#[tokio::test]
async fn static_known_child_resolves() {
    let hub = StaticOnlySolar {
        mercury: Mercury { tag: "mercury-static" },
        venus: Venus { tag: "venus-static" },
    };
    let got = hub.get_child("mercury").await;
    assert!(got.is_some(), "expected Some for known static child");
    assert_eq!(got.unwrap().router_namespace(), "mercury");
}

#[tokio::test]
async fn static_unknown_child_returns_none() {
    let hub = StaticOnlySolar {
        mercury: Mercury { tag: "mercury-static" },
        venus: Venus { tag: "venus-static" },
    };
    assert!(hub.get_child("not_a_child").await.is_none());
}

#[tokio::test]
async fn generated_router_advertises_no_optional_capabilities() {
    let hub = StaticOnlySolar {
        mercury: Mercury { tag: "mercury-static" },
        venus: Venus { tag: "venus-static" },
    };
    assert_eq!(hub.capabilities(), ChildCapabilities::empty());
}

#[tokio::test]
async fn generated_router_exposes_activation_namespace() {
    let hub = StaticOnlySolar {
        mercury: Mercury { tag: "mercury-static" },
        venus: Venus { tag: "venus-static" },
    };
    assert_eq!(hub.router_namespace(), "static_solar");
}

// ---------------------------------------------------------------------------
// Case 2: static + dynamic mix. Dynamic children returned carry distinct
// tags so the test can prove the dynamic arm fired.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DynamicSolar {
    mercury: Mercury,
}

#[plexus_macros::activation(
    namespace = "dynamic_solar",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl DynamicSolar {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// The Mercury child — must win over the dynamic fallback.
    #[plexus_macros::child]
    fn mercury(&self) -> Mercury {
        self.mercury.clone()
    }

    /// Dynamic fallback: accepts names starting with `planet_`, rejects
    /// the literal `reject_me`.
    #[plexus_macros::child]
    async fn planet(&self, name: &str) -> Option<Mercury> {
        if name == "reject_me" {
            None
        } else if let Some(tail) = name.strip_prefix("planet_") {
            // Leak the tail into a &'static str so the tag is distinct.
            let tag: &'static str = Box::leak(format!("dynamic:{}", tail).into_boxed_str());
            Some(Mercury { tag })
        } else {
            None
        }
    }
}

#[tokio::test]
async fn static_arm_wins_over_dynamic_arm() {
    let hub = DynamicSolar { mercury: Mercury { tag: "static-mercury" } };
    let got = hub.get_child("mercury").await.expect("static arm returns Some");
    // The static arm returns the literal field clone; its namespace is "mercury".
    assert_eq!(got.router_namespace(), "mercury");
}

#[tokio::test]
async fn unknown_name_routes_through_dynamic_arm() {
    let hub = DynamicSolar { mercury: Mercury { tag: "static-mercury" } };
    let got = hub
        .get_child("planet_42")
        .await
        .expect("dynamic arm produces Some");
    // Dynamic children are Mercury leaves too; their namespace is "mercury".
    // The distinguishing factor is the `tag` field, which is inaccessible
    // through `Box<dyn ChildRouter>`. router_namespace() is sufficient to
    // prove that routing succeeded through the dynamic arm (the static
    // arm would have returned None for `planet_42`).
    assert_eq!(got.router_namespace(), "mercury");
}

#[tokio::test]
async fn dynamic_arm_can_return_none() {
    let hub = DynamicSolar { mercury: Mercury { tag: "static-mercury" } };
    assert!(hub.get_child("reject_me").await.is_none());
}

// ---------------------------------------------------------------------------
// Case 3: dynamic-only (no static children). Proves the pure-dynamic
// code path compiles and dispatches correctly.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DynamicOnly;

#[plexus_macros::activation(
    namespace = "dynamic_only",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl DynamicOnly {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// Always produce a Mercury for any name.
    #[plexus_macros::child]
    fn any(&self, _name: &str) -> Option<Mercury> {
        Some(Mercury { tag: "dynamic-only" })
    }
}

#[tokio::test]
async fn dynamic_only_accepts_any_name() {
    let hub = DynamicOnly;
    let got = hub.get_child("whatever").await.expect("dynamic-only returns Some");
    assert_eq!(got.router_namespace(), "mercury");
}
