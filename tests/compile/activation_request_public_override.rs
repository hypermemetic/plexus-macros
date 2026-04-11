//! REQ-4: #[plexus::method(request = ())] public override — must compile.
//! health() is callable without cookies; protected() requires extraction.

use async_stream::stream;
use futures::stream::Stream;
use plexus_macros::PlexusRequest;

#[derive(PlexusRequest)]
struct TestRequest {
    #[from_cookie("access_token")]
    auth_token: String,
}

struct TestHub;

#[plexus_macros::activation(namespace = "test", version = "1.0.0", request = TestRequest, crate_path = "plexus_core")]
impl TestHub {
    #[plexus_macros::method(description = "protected")]
    async fn protected(
        &self,
        #[activation_param] auth_token: String,
    ) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield auth_token; }
    }

    #[plexus_macros::method(description = "health", request = ())]
    async fn health(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "ok".into(); }
    }
}

fn main() {}
