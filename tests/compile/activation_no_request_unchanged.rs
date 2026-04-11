//! REQ-4: Existing activation with no request = arg — must compile unchanged.
//! Backward compatibility: existing code must not require changes.

use async_stream::stream;
use futures::stream::Stream;

struct TestHub;

#[plexus_macros::activation(namespace = "test", version = "1.0.0", crate_path = "plexus_core")]
impl TestHub {
    #[plexus_macros::method(description = "plain")]
    async fn plain(&self, x: i32) -> impl Stream<Item = i32> + Send + 'static {
        stream! { yield x; }
    }
}

fn main() {}
