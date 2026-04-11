//! REQ-2: #[from_auth(expr)] backward compatibility — must compile.

use async_stream::stream;
use futures::stream::Stream;
use plexus_macros::PlexusRequest;

#[derive(PlexusRequest)]
struct TestRequest {
    #[from_cookie("access_token")]
    auth_token: String,
    #[from_auth_context]
    auth: Option<plexus_core::plexus::AuthContext>,
}

struct Db;
struct ValidUser { pub id: String }
impl Db {
    async fn validate_user(&self, _auth: &plexus_core::plexus::AuthContext) -> Result<ValidUser, String> {
        Ok(ValidUser { id: "u1".into() })
    }
}

struct TestHub { db: Db }

#[plexus_macros::activation(namespace = "test", version = "1.0.0", request = TestRequest, crate_path = "plexus_core")]
impl TestHub {
    #[plexus_macros::method(description = "authed")]
    async fn authed(
        &self,
        #[from_auth(self.db.validate_user)] user: ValidUser,
    ) -> impl Stream<Item = String> + Send + 'static {
        let id = user.id.clone();
        stream! { yield id; }
    }
}

fn main() {}
