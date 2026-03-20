mod auth;
mod collections;
mod globals;
mod jobs;
pub mod openapi;
mod proxy;
mod schema;
mod subscribe;
mod versions;

use axum::{Router, extract::FromRef, http::HeaderMap};

use crate::client::GrpcClient;
use crate::config::{OpenApiConfig, SubscribeConfig};

#[derive(Clone)]
pub struct AppState {
    pub grpc: GrpcClient,
    pub proxy: Option<ProxyState>,
    pub subscribe: Option<SubscribeConfig>,
}

#[derive(Clone)]
pub struct ProxyState {
    pub client: reqwest::Client,
    pub cms_url: String,
}

impl FromRef<AppState> for GrpcClient {
    fn from_ref(state: &AppState) -> Self {
        state.grpc.clone()
    }
}

/// Build a gRPC request with the `authorization` metadata forwarded from the
/// incoming HTTP headers. Used by all handler modules.
pub(crate) fn make_request<T>(headers: &HeaderMap, msg: T) -> tonic::Request<T> {
    let mut req = tonic::Request::new(msg);
    if let Some(auth) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
    {
        req.metadata_mut().insert("authorization", auth);
    }
    req
}

pub fn router(state: AppState, openapi_config: &OpenApiConfig) -> Router {
    let main = Router::new()
        .merge(collections::routes())
        .merge(globals::routes())
        .merge(auth::routes())
        .merge(schema::routes())
        .merge(versions::routes())
        .merge(jobs::routes());

    let main = if state.proxy.is_some() {
        main.merge(proxy::routes())
    } else {
        main
    };

    let main = if state.subscribe.is_some() {
        main.merge(subscribe::routes())
    } else {
        main
    };

    let grpc = state.grpc.clone();
    let main = main.with_state(state);

    main.merge(openapi::routes(grpc, openapi_config))
}
