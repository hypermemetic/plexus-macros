//! REQ-2: #[activation_param] field injection — must compile.
//! The derive pulls named fields from the activation's request struct.

use async_stream::stream;
use futures::stream::Stream;
use plexus_macros::PlexusRequest;

#[derive(PlexusRequest)]
struct TestRequest {
    #[from_cookie("access_token")]
    auth_token: String,
    #[from_header("origin")]
    origin: Option<String>,
}

struct TestHub;

#[plexus_macros::activation(namespace = "test", version = "1.0.0", request = TestRequest, crate_path = "plexus_core")]
impl TestHub {
    #[plexus_macros::method(description = "echo")]
    async fn echo(
        &self,
        #[activation_param] auth_token: String,
        #[activation_param] origin: Option<String>,
    ) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield format!("{} {:?}", auth_token, origin); }
    }
}

fn main() {}
