//! REQ-2: #[activation_param] with wrong type — must FAIL with type error.
//! auth_token is String in TestRequest but declared as u32 here.

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
