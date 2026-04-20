//! IR-3 AC #6: `#[plexus_macros::removed_in("...")]` on a method that is NOT
//! also `#[deprecated]` must fail to compile. The error message names
//! `#[deprecated]` as the required companion attribute.

use futures::stream::Stream;

#[derive(Clone)]
struct Hub;

#[plexus_macros::activation(
    namespace = "removed_in_without_deprecated",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
impl Hub {
    #[plexus_macros::removed_in("0.6")]
    #[plexus_macros::method]
    async fn orphan(&self) -> impl Stream<Item = String> + Send + 'static {
        async_stream::stream! { yield "never".into(); }
    }
}

fn main() {}
