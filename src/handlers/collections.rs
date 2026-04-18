use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{delete, get, patch, post},
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::{document_to_json, json_to_struct};
use crate::error::RestResult;
use crate::proto;

use super::{AppState, make_request};

#[derive(Debug, Deserialize, Default)]
pub struct FindParams {
    pub r#where: Option<String>,
    pub order_by: Option<String>,
    pub limit: Option<i64>,
    pub page: Option<i64>,
    pub depth: Option<i32>,
    pub locale: Option<String>,
    pub select: Option<String>,
    pub draft: Option<bool>,
    pub after_cursor: Option<String>,
    pub before_cursor: Option<String>,
    pub search: Option<String>,
    pub trash: Option<bool>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/collections/{slug}", get(find))
        .route("/collections/{slug}/count", get(count))
        .route("/collections/{slug}/{id}", get(find_by_id))
        .route("/collections/{slug}", post(create))
        .route("/collections/{slug}/{id}", patch(update))
        .route("/collections/{slug}/{id}", delete(delete_doc))
        .route("/collections/{slug}/{id}/undelete", post(undelete))
        .route("/collections/{slug}/validate", post(validate))
        .route("/collections/{slug}/bulk", post(create_many))
        .route("/collections/{slug}/bulk", patch(update_many))
        .route("/collections/{slug}/bulk", delete(delete_many))
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
            page: params.page,
            depth: params.depth,
            locale: params.locale,
            select,
            draft: params.draft,
            after_cursor: params.after_cursor,
            before_cursor: params.before_cursor,
            search: params.search,
            trash: params.trash,
        },
    );

    let resp = client.client().find(req).await?.into_inner();
    let docs: Vec<Value> = resp.documents.iter().map(document_to_json).collect();
    let pg = resp.pagination.unwrap_or_default();

    let mut pagination = serde_json::json!({
        "totalDocs": pg.total_docs,
        "limit": pg.limit,
        "hasPrevPage": pg.has_prev_page,
        "hasNextPage": pg.has_next_page,
    });
    if let Some(tp) = pg.total_pages {
        pagination["totalPages"] = serde_json::json!(tp);
    }
    if let Some(p) = pg.page {
        pagination["page"] = serde_json::json!(p);
    }
    if let Some(ps) = pg.page_start {
        pagination["pageStart"] = serde_json::json!(ps);
    }
    if let Some(prev) = pg.prev_page {
        pagination["prevPage"] = serde_json::json!(prev);
    }
    if let Some(next) = pg.next_page {
        pagination["nextPage"] = serde_json::json!(next);
    }
    if let Some(ref sc) = pg.start_cursor {
        pagination["startCursor"] = serde_json::json!(sc);
    }
    if let Some(ref ec) = pg.end_cursor {
        pagination["endCursor"] = serde_json::json!(ec);
    }

    Ok(Json(serde_json::json!({
        "docs": docs,
        "pagination": pagination,
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
            search: params.search,
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
            trash: params.trash,
        },
    );

    let resp = client.client().find_by_id(req).await?.into_inner();
    let doc = resp
        .document
        .ok_or_else(|| tonic::Status::not_found("document not found"))?;
    Ok(Json(document_to_json(&doc)))
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
            locale: body
                .get("_locale")
                .and_then(|v| v.as_str())
                .map(String::from),
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
            locale: body
                .get("_locale")
                .and_then(|v| v.as_str())
                .map(String::from),
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

#[derive(Debug, Deserialize, Default)]
pub struct DeleteParams {
    pub force: Option<bool>,
}

async fn delete_doc(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    Query(params): Query<DeleteParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::DeleteRequest {
            collection: slug,
            id,
            force_hard_delete: params.force.unwrap_or(false),
        },
    );

    let resp = client.client().delete(req).await?.into_inner();
    Ok(Json(serde_json::json!({
        "success": resp.success,
        "softDeleted": resp.soft_deleted,
    })))
}

async fn undelete(
    State(client): State<GrpcClient>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::UndeleteRequest {
            collection: slug,
            id,
        },
    );

    let resp = client.client().undelete(req).await?.into_inner();
    match resp.document {
        Some(doc) => Ok(Json(document_to_json(&doc))),
        None => Ok(Json(serde_json::json!({}))),
    }
}

async fn validate(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let data = json_to_struct(&body);
    let req = make_request(
        &headers,
        proto::ValidateRequest {
            collection: slug,
            data,
            draft: body.get("_draft").and_then(|v| v.as_bool()),
            locale: body
                .get("_locale")
                .and_then(|v| v.as_str())
                .map(String::from),
            id: body.get("_id").and_then(|v| v.as_str()).map(String::from),
        },
    );

    let resp = client.client().validate(req).await?.into_inner();
    Ok(Json(serde_json::json!({
        "valid": resp.valid,
        "errors": resp.errors,
    })))
}

async fn create_many(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let documents = body
        .get("documents")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(json_to_struct).collect())
        .unwrap_or_default();

    let req = make_request(
        &headers,
        proto::CreateManyRequest {
            collection: slug,
            documents,
            locale: body
                .get("locale")
                .and_then(|v| v.as_str())
                .map(String::from),
            draft: body.get("draft").and_then(|v| v.as_bool()),
            hooks: body.get("hooks").and_then(|v| v.as_bool()),
        },
    );

    let resp = client.client().create_many(req).await?.into_inner();
    let docs: Vec<Value> = resp.documents.iter().map(document_to_json).collect();

    Ok(Json(serde_json::json!({
        "created": resp.created,
        "documents": docs,
    })))
}

async fn update_many(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> RestResult<Json<Value>> {
    let where_clause = body.get("where").and_then(|v| v.as_str()).map(String::from);

    let data_val = body
        .get("data")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    let data = json_to_struct(&data_val);

    let req = make_request(
        &headers,
        proto::UpdateManyRequest {
            collection: slug,
            r#where: where_clause,
            data,
            locale: body
                .get("locale")
                .and_then(|v| v.as_str())
                .map(String::from),
            draft: body.get("draft").and_then(|v| v.as_bool()),
            hooks: body.get("hooks").and_then(|v| v.as_bool()),
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
    let where_clause = body.get("where").and_then(|v| v.as_str()).map(String::from);

    let req = make_request(
        &headers,
        proto::DeleteManyRequest {
            collection: slug,
            r#where: where_clause,
            hooks: body.get("hooks").and_then(|v| v.as_bool()),
            force_hard_delete: body
                .get("force_hard_delete")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
    );

    let resp = client.client().delete_many(req).await?.into_inner();
    let mut result = serde_json::json!({ "deleted": resp.deleted });
    if resp.soft_deleted > 0 {
        result["soft_deleted"] = serde_json::json!(resp.soft_deleted);
    }
    if resp.skipped > 0 {
        result["skipped"] = serde_json::json!(resp.skipped);
    }
    Ok(Json(result))
}
