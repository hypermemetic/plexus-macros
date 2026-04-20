//! CHILD-3 acceptance criterion 6: two dynamic `#[child]` methods on the
//! same impl must fail to compile, naming the conflicting methods.

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

    #[plexus_macros::child]
    fn first(&self, _name: &str) -> Option<Leaf> {
        None
    }

    #[plexus_macros::child]
    fn second(&self, _name: &str) -> Option<Leaf> {
        None
    }
}

fn main() {}
