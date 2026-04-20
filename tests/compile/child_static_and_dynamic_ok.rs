//! CHILD-3: an activation with one static and one dynamic `#[child]`
//! method must compile cleanly — verifying the happy path produces valid
//! code with no stray warnings that would fail trybuild in strict mode.

use async_stream::stream;
use futures::stream::Stream;

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

#[derive(Clone)]
struct Solar {
    mercury: Leaf,
}

#[plexus_macros::activation(
    namespace = "solar",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl Solar {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    /// Static child — routed by its method name.
    #[plexus_macros::child]
    fn mercury(&self) -> Leaf {
        self.mercury.clone()
    }

    /// Dynamic fallback — receives the unmatched name.
    #[plexus_macros::child]
    async fn planet(&self, _name: &str) -> Option<Leaf> {
        Some(Leaf)
    }
}

fn main() {}
