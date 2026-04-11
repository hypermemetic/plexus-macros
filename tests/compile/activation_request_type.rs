//! REQ-4: activation with request = Type — must compile.
//! Methods without #[activation_param] are still protected by extraction failure.

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
    #[plexus_macros::method(description = "list")]
    async fn list(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "ok".into(); }
    }

    #[plexus_macros::method(description = "get")]
    async fn get(&self, id: String) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield id; }
    }
}

fn main() {}
