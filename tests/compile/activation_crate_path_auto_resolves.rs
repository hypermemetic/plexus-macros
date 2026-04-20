//! CHILD-6 acceptance criterion: #[plexus_macros::activation] without an
//! explicit `crate_path` auto-resolves to the invoking crate's dependency
//! name for `plexus-core` via `proc-macro-crate`.
//!
//! This fixture is compiled from plexus-macros itself (which dev-depends on
//! `plexus-core` under its canonical name), so the generated code must
//! resolve to `::plexus_core::...`.

use async_stream::stream;
use futures::stream::Stream;

struct TestHub;

#[plexus_macros::activation(namespace = "test", version = "1.0.0")]
impl TestHub {
    #[plexus_macros::method(description = "plain")]
    async fn plain(&self, x: i32) -> impl Stream<Item = i32> + Send + 'static {
        stream! { yield x; }
    }
}

fn main() {}
