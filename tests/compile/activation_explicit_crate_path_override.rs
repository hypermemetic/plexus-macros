//! CHILD-6 acceptance criterion 5: explicit `crate_path = "..."` still
//! overrides the default resolver. We alias the plexus_core crate locally
//! as `custom_alias` and confirm the macro uses the override verbatim.

use async_stream::stream;
use futures::stream::Stream;

// Local alias for plexus_core so the explicit `crate_path = "custom_alias"`
// in the macro invocation has a target to resolve against.
use plexus_core as custom_alias;

struct OverrideHub;

#[plexus_macros::activation(
    namespace = "test",
    version = "1.0.0",
    crate_path = "custom_alias"
)]
impl OverrideHub {
    #[plexus_macros::method(description = "plain")]
    async fn plain(&self, x: i32) -> impl Stream<Item = i32> + Send + 'static {
        stream! { yield x; }
    }
}

fn main() {}
