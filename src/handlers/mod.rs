mod auth;
mod collections;
mod globals;
mod jobs;
mod schema;
mod versions;

use axum::Router;

use crate::client::GrpcClient;

pub fn router(client: GrpcClient) -> Router {
    Router::new()
        .merge(collections::routes())
        .merge(globals::routes())
        .merge(auth::routes())
        .merge(schema::routes())
        .merge(versions::routes())
        .merge(jobs::routes())
        .with_state(client)
}
