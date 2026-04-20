//! CHILD-3 acceptance criterion 4: a `#[child]` method whose parameter
//! is not `name: &str` (here `name: u32`) must fail to compile, and the
//! error must contain the phrase `child method signature`.

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

    // Unsupported shape: the dynamic child form requires `name: &str`.
    #[plexus_macros::child]
    fn planet(&self, name: u32) -> Option<Leaf> {
        let _ = name;
        None
    }
}

fn main() {}
