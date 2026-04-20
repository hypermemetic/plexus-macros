//! CHILD-3 acceptance criterion 5: `#[child]` and `#[method]` on the same
//! function must fail to compile. The error contains `mutually exclusive`.

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

struct Solar;

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

    // A function cannot simultaneously be an RPC method and a child router entry.
    #[plexus_macros::child]
    #[plexus_macros::method]
    async fn mercury(&self) -> impl Stream<Item = Leaf> + Send + 'static {
        stream! { yield Leaf; }
    }
}

fn main() {}
