mod auth;
mod collections;
mod globals;
mod jobs;
pub mod openapi;
mod schema;
mod versions;

use axum::{http::HeaderMap, Router};

use crate::client::GrpcClient;
use crate::config::OpenApiConfig;

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

pub fn router(client: GrpcClient, openapi_config: &OpenApiConfig) -> Router {
    let main = Router::new()
        .merge(collections::routes())
        .merge(globals::routes())
        .merge(auth::routes())
        .merge(schema::routes())
        .merge(versions::routes())
        .merge(jobs::routes())
        .with_state(client.clone());

    main.merge(openapi::routes(client, openapi_config))
}
