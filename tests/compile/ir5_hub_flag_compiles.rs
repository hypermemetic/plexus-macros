//! IR-5 AC #6: `#[plexus_macros::activation(hub)]` continues to compile but
//! emits a build warning pointing at the replacement syntax
//! (`#[plexus_macros::child]`-tagged methods).
//!
//! This fixture is a `trybuild::TestCases::pass` test — its job is to prove
//! the activation attribute with the `hub` flag still accepts. The warning
//! text itself is asserted by `ir5_warnings_tests::hub_flag_emits_warning`
//! which runs `cargo build` on a separate scratch crate and greps stderr.

use async_stream::stream;
use futures::stream::Stream;

#[derive(Clone)]
struct HubFlagActivation;

// Hand-written ChildRouter so the legacy `hub` flag works without `#[child]`.
#[async_trait::async_trait]
impl plexus_core::plexus::ChildRouter for HubFlagActivation {
    fn router_namespace(&self) -> &str {
        "hub_flag"
    }
    async fn router_call(
        &self,
        method: &str,
        params: serde_json::Value,
        auth: Option<&plexus_core::plexus::AuthContext>,
        raw_ctx: Option<&plexus_core::request::RawRequestContext>,
    ) -> Result<plexus_core::plexus::PlexusStream, plexus_core::plexus::PlexusError> {
        <Self as plexus_core::plexus::Activation>::call(self, method, params, auth, raw_ctx).await
    }
    async fn get_child(
        &self,
        _name: &str,
    ) -> Option<Box<dyn plexus_core::plexus::ChildRouter>> {
        None
    }
}

#[plexus_macros::activation(
    namespace = "hub_flag",
    version = "1.0.0",
    hub,
    crate_path = "plexus_core"
)]
impl HubFlagActivation {
    #[plexus_macros::method]
    async fn ping(&self) -> impl Stream<Item = String> + Send + 'static {
        stream! { yield "pong".into(); }
    }

    pub fn plugin_children(&self) -> Vec<plexus_core::plexus::ChildSummary> {
        Vec::new()
    }
}

fn main() {}
