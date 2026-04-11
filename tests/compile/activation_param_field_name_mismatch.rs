//! REQ-2: #[activation_param] with field name not in struct — must FAIL.
//! Expected: compile error mentioning "nonexistent" not found in TestRequest.

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
    #[plexus_macros::method(description = "bad")]
    async fn bad(
        &self,
        #[activation_param] nonexistent: String,
    ) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "x".into(); }
    }
}

fn main() {}
