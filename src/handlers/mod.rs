mod auth;
mod collections;
mod globals;
mod jobs;
pub mod openapi;
mod schema;
mod versions;

use axum::Router;

use crate::client::GrpcClient;
use crate::config::OpenApiConfig;

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
