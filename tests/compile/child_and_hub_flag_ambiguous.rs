//! CHILD-8: mixing the explicit `hub` flag with `#[plexus_macros::child]`
//! methods must be rejected at compile time — both mechanisms declare the
//! same "this activation is a hub" fact and picking one silently would
//! hide intent.

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
struct Mixed {
    leaf: Leaf,
}

#[plexus_macros::activation(
    namespace = "mixed",
    version = "1.0.0",
    hub,
    crate_path = "plexus_core"
)]
impl Mixed {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    #[plexus_macros::child]
    fn mercury(&self) -> Leaf {
        self.leaf.clone()
    }
}

fn main() {}
