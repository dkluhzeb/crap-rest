use std::sync::Arc;

use tonic::transport::{Channel, Endpoint};

use crate::proto::content_api_client::ContentApiClient;

#[derive(Clone)]
pub struct GrpcClient {
    inner: Arc<ContentApiClient<Channel>>,
}

impl GrpcClient {
    /// Create a client with a lazy channel — does not connect until the first RPC.
    pub fn new(addr: &str) -> anyhow::Result<Self> {
        let channel = Endpoint::from_shared(addr.to_string())?.connect_lazy();
        let client = ContentApiClient::new(channel);
        Ok(Self {
            inner: Arc::new(client),
        })
    }

    /// Get a cloned client handle for making requests.
    /// Tonic clients are cheap to clone (they share the underlying channel).
    pub fn client(&self) -> ContentApiClient<Channel> {
        (*self.inner).clone()
    }
}
