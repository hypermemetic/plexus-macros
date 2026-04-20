//! IR-20: a generic activation using `#[plexus_macros::child]` must compile.
//!
//! Regression fixture for the bug where the synthesized `plugin_children()`
//! impl block was non-generic, so `Activation for Orcha<P>` (generic) could
//! not call `self.plugin_children()` on arbitrary `P` (E0599).
//!
//! The fix propagates `#impl_generics` + `#self_ty` + `#where_clause` into
//! the synthesized impl block so it matches the Activation impl's generics.

use async_stream::stream;
use futures::stream::Stream;

#[derive(Clone, Default)]
struct LeafChild;

#[plexus_macros::activation(
    namespace = "leaf_child",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl LeafChild {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }
}

#[derive(Clone, Default)]
struct GenericFixture<T: Default + Clone + Send + Sync + 'static> {
    _marker: std::marker::PhantomData<T>,
}

#[plexus_macros::activation(
    namespace = "gen_fixture",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl<T: Default + Clone + Send + Sync + 'static> GenericFixture<T> {
    #[plexus_macros::method]
    async fn noop(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "ok".into(); }
    }

    /// Static child — ensures synthesized `plugin_children()` lands on the
    /// same generic impl as the `Activation` impl so the trait method can
    /// call `self.plugin_children()` on arbitrary `T`.
    #[plexus_macros::child]
    fn child(&self) -> LeafChild {
        LeafChild::default()
    }
}

fn main() {
    // Exercise instantiation with a concrete `T` so the compiler emits the
    // monomorphized `Activation` impl and we catch E0599 at build time.
    let _fixture: GenericFixture<()> = GenericFixture::default();
}
