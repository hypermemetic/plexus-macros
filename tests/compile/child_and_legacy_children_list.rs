//! CHILD-3 acceptance criterion 7: combining the new `#[child]` method
//! attribute with the legacy `children = [...]` activation attribute on the
//! same impl must fail to compile — forcing migration clarity.

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
    leaf: Leaf,
}

#[plexus_macros::activation(
    namespace = "solar",
    version = "1.0.0",
    crate_path = "plexus_core",
    children(leaf)
)]
impl Solar {
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
