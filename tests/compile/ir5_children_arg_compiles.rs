//! IR-5 AC #7: `#[plexus_macros::activation(children = [...])]` continues to
//! compile but emits a build warning pointing at the replacement syntax
//! (`#[plexus_macros::child]` on accessor methods).

use async_stream::stream;
use futures::stream::Stream;

#[derive(Clone)]
struct InnerLeaf;

#[plexus_macros::activation(
    namespace = "inner",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl InnerLeaf {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "inner".into(); }
    }
}

#[derive(Clone)]
struct ChildrenArgActivation {
    inner: InnerLeaf,
}

#[plexus_macros::activation(
    namespace = "children_arg",
    version = "1.0.0",
    children = [inner],
    crate_path = "plexus_core"
)]
impl ChildrenArgActivation {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "parent".into(); }
    }
}

fn main() {}
