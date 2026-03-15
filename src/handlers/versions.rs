use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::document_to_json;
use crate::error::RestResult;
use crate::proto;

use super::make_request;

#[derive(Debug, Deserialize, Default)]
pub struct VersionParams {
    pub limit: Option<i64>,
}

pub fn routes() -> Router<GrpcClient> {
    Router::new()
        .route("/api/collections/{slug}/{id}/versions", get(list_versions))
        .route(
            "/api/collections/{slug}/{id}/versions/{vid}/restore",
            post(restore_version),
        )
}

async fn list_versions(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    Query(params): Query<VersionParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::ListVersionsRequest {
            collection: slug,
            id,
            limit: params.limit,
        },
    );

    let resp = client.client().list_versions(req).await?.into_inner();
    let versions: Vec<Value> = resp
        .versions
        .iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id,
                "version": v.version,
                "status": v.status,
                "latest": v.latest,
                "created_at": v.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "versions": versions })))
}

async fn restore_version(
    State(client): State<GrpcClient>,
    Path((slug, id, vid)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::RestoreVersionRequest {
            collection: slug,
            document_id: id,
            version_id: vid,
        },
    );

    let resp = client.client().restore_version(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}
