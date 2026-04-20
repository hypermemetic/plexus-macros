//! IR-3 AC #4 (parse-only): applying `#[deprecated(since = "...", note = "...")]`
//! to an `#[activation]` impl block must compile cleanly. Emitting the
//! activation-level deprecation onto the `PluginSchema` is IR-5's scope; this
//! fixture only verifies the parse-and-codegen path doesn't reject the
//! attribute today.

use async_stream::stream;
use futures::stream::Stream;

#[derive(Clone)]
struct Hub;

#[deprecated(since = "0.5", note = "migrate to FutureHub")]
#[plexus_macros::activation(
    namespace = "deprecated_activation",
    version = "1.0.0",
    crate_path = "plexus_core"
)]
#[allow(deprecated)]
impl Hub {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }
}

fn main() {
    // The generated `plugin_id()` inherent function only proves the macro ran.
    #[allow(deprecated)]
    let _hub = Hub;
}
