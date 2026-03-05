use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::{document_to_json, json_to_struct};
use crate::error::RestResult;
use crate::proto;

#[derive(Debug, Deserialize, Default)]
pub struct FindParams {
    pub r#where: Option<String>,
    pub order_by: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub depth: Option<i32>,
    pub locale: Option<String>,
    pub select: Option<String>,
    pub draft: Option<bool>,
}

fn make_request<T>(headers: &HeaderMap, msg: T) -> tonic::Request<T> {
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

pub fn routes() -> Router<GrpcClient> {
    Router::new()
        .route("/api/collections/{slug}", get(find))
        .route("/api/collections/{slug}/count", get(count))
        .route("/api/collections/{slug}/{id}", get(find_by_id))
        .route("/api/collections/{slug}", post(create))
        .route("/api/collections/{slug}/{id}", patch(update))
        .route("/api/collections/{slug}/{id}", delete(delete_doc))
        .route("/api/collections/{slug}/bulk", patch(update_many))
        .route("/api/collections/{slug}/bulk", delete(delete_many))
}

async fn find(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    Query(params): Query<FindParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let select: Vec<String> = params
        .select
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_default();

    let req = make_request(
        &headers,
        proto::FindRequest {
            collection: slug,
            r#where: params.r#where,
            order_by: params.order_by,
            limit: params.limit,
            offset: params.offset,
            depth: params.depth,
            locale: params.locale,
            select,
            draft: params.draft,
        },
    );

    let resp = client.client().find(req).await?.into_inner();
    let docs: Vec<Value> = resp.documents.iter().map(document_to_json).collect();

    Ok(Json(serde_json::json!({
        "docs": docs,
        "total": resp.total,
    })))
}

async fn count(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    Query(params): Query<FindParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::CountRequest {
            collection: slug,
            r#where: params.r#where,
            locale: params.locale,
            draft: params.draft,
        },
    );

    let resp = client.client().count(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "count": resp.count })))
}

async fn find_by_id(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    Query(params): Query<FindParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let select: Vec<String> = params
        .select
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_default();

    let req = make_request(
        &headers,
        proto::FindByIdRequest {
            collection: slug,
            id,
            depth: params.depth,
            locale: params.locale,
            select,
            draft: params.draft,
        },
    );

    let resp = client.client().find_by_id(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Err(tonic::Status::not_found("document not found").into()),
    }
}

async fn create(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let data = json_to_struct(&body);
    let req = make_request(
        &headers,
        proto::CreateRequest {
            collection: slug,
            data,
            locale: body.get("_locale").and_then(|v| v.as_str()).map(String::from),
            draft: body.get("_draft").and_then(|v| v.as_bool()),
        },
    );

    let resp = client.client().create(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}

async fn update(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let data = json_to_struct(&body);
    let req = make_request(
        &headers,
        proto::UpdateRequest {
            collection: slug,
            id,
            data,
            locale: body.get("_locale").and_then(|v| v.as_str()).map(String::from),
            draft: body.get("_draft").and_then(|v| v.as_bool()),
            unpublish: body.get("_unpublish").and_then(|v| v.as_bool()),
        },
    );

    let resp = client.client().update(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}

async fn delete_doc(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::DeleteRequest {
            collection: slug,
            id,
        },
    );

    let resp = client.client().delete(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

async fn update_many(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let where_clause = body
        .get("where")
        .and_then(|v| v.as_str())
        .map(String::from);

    let data_val = body.get("data").cloned().unwrap_or(Value::Object(Default::default()));
    let data = json_to_struct(&data_val);

    let req = make_request(
        &headers,
        proto::UpdateManyRequest {
            collection: slug,
            r#where: where_clause,
            data,
            locale: body.get("locale").and_then(|v| v.as_str()).map(String::from),
            draft: body.get("draft").and_then(|v| v.as_bool()),
        },
    );

    let resp = client.client().update_many(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "modified": resp.modified })))
}

async fn delete_many(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let where_clause = body
        .get("where")
        .and_then(|v| v.as_str())
        .map(String::from);

    let req = make_request(
        &headers,
        proto::DeleteManyRequest {
            collection: slug,
            r#where: where_clause,
        },
    );

    let resp = client.client().delete_many(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "deleted": resp.deleted })))
}
