use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, patch},
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::{document_to_json, json_to_struct};
use crate::error::RestResult;
use crate::proto;

use super::{AppState, make_request};

#[derive(Debug, Deserialize, Default)]
pub struct GlobalParams {
    pub locale: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/globals/{slug}", get(get_global))
        .route("/api/globals/{slug}", patch(update_global))
}

async fn get_global(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    axum::extract::Query(params): axum::extract::Query<GlobalParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::GetGlobalRequest {
            slug,
            locale: params.locale,
        },
    );

    let resp = client.client().get_global(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}

async fn update_global(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let data = json_to_struct(&body);
    let req = make_request(
        &headers,
        proto::UpdateGlobalRequest {
            slug,
            data,
            locale: body
                .get("_locale")
                .and_then(|v| v.as_str())
                .map(String::from),
        },
    );

    let resp = client.client().update_global(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}
