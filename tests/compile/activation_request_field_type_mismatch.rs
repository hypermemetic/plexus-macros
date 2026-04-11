//! REQ-4: #[activation_param] with wrong type in activation-level context — must FAIL.
//! TestRequest has auth_token: String; method declares u32 → type error.

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
        #[activation_param] auth_token: u32,
    ) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "x".into(); }
    }
}

fn main() {}
