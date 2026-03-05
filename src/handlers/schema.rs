use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde_json::Value;

use crate::client::GrpcClient;
use crate::error::RestResult;
use crate::proto;

pub fn routes() -> Router<GrpcClient> {
    Router::new()
        .route("/api/schema", get(list_collections))
        .route("/api/schema/collections/{slug}", get(describe_collection))
        .route("/api/schema/globals/{slug}", get(describe_global))
}

async fn list_collections(
    State(client): State<GrpcClient>,
) -> RestResult<Json<Value>> {
    let req = tonic::Request::new(proto::ListCollectionsRequest {});
    let resp = client.client().list_collections(req).await?.into_inner();

    let collections: Vec<Value> = resp
        .collections
        .iter()
        .map(|c| {
            serde_json::json!({
                "slug": c.slug,
                "singular_label": c.singular_label,
                "plural_label": c.plural_label,
                "timestamps": c.timestamps,
                "auth": c.auth,
                "upload": c.upload,
            })
        })
        .collect();

    let globals: Vec<Value> = resp
        .globals
        .iter()
        .map(|g| {
            serde_json::json!({
                "slug": g.slug,
                "singular_label": g.singular_label,
                "plural_label": g.plural_label,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "collections": collections,
        "globals": globals,
    })))
}

fn field_info_to_json(f: &proto::FieldInfo) -> Value {
    let mut obj = serde_json::json!({
        "name": f.name,
        "type": f.r#type,
        "required": f.required,
        "unique": f.unique,
        "localized": f.localized,
    });

    if let Some(ref col) = f.relationship_collection {
        obj["relationship_collection"] = Value::String(col.clone());
    }
    if let Some(hm) = f.relationship_has_many {
        obj["relationship_has_many"] = Value::Bool(hm);
    }
    if let Some(md) = f.relationship_max_depth {
        obj["relationship_max_depth"] = Value::Number(md.into());
    }
    if !f.options.is_empty() {
        obj["options"] = Value::Array(
            f.options
                .iter()
                .map(|o| {
                    serde_json::json!({
                        "label": o.label,
                        "value": o.value,
                    })
                })
                .collect(),
        );
    }
    if !f.fields.is_empty() {
        obj["fields"] = Value::Array(f.fields.iter().map(field_info_to_json).collect());
    }
    if !f.blocks.is_empty() {
        obj["blocks"] = Value::Array(
            f.blocks
                .iter()
                .map(|b| {
                    let mut block = serde_json::json!({
                        "block_type": b.block_type,
                    });
                    if let Some(ref label) = b.label {
                        block["label"] = Value::String(label.clone());
                    }
                    if let Some(ref group) = b.group {
                        block["group"] = Value::String(group.clone());
                    }
                    if let Some(ref url) = b.image_url {
                        block["image_url"] = Value::String(url.clone());
                    }
                    if !b.fields.is_empty() {
                        block["fields"] =
                            Value::Array(b.fields.iter().map(field_info_to_json).collect());
                    }
                    block
                })
                .collect(),
        );
    }

    obj
}

async fn describe_collection(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
) -> RestResult<Json<Value>> {
    let req = tonic::Request::new(proto::DescribeCollectionRequest {
        slug,
        is_global: false,
    });

    let resp = client.client().describe_collection(req).await?.into_inner();
    let fields: Vec<Value> = resp.fields.iter().map(field_info_to_json).collect();

    Ok(Json(serde_json::json!({
        "slug": resp.slug,
        "singular_label": resp.singular_label,
        "plural_label": resp.plural_label,
        "timestamps": resp.timestamps,
        "auth": resp.auth,
        "upload": resp.upload,
        "drafts": resp.drafts,
        "fields": fields,
    })))
}

async fn describe_global(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
) -> RestResult<Json<Value>> {
    let req = tonic::Request::new(proto::DescribeCollectionRequest {
        slug,
        is_global: true,
    });

    let resp = client.client().describe_collection(req).await?.into_inner();
    let fields: Vec<Value> = resp.fields.iter().map(field_info_to_json).collect();

    Ok(Json(serde_json::json!({
        "slug": resp.slug,
        "singular_label": resp.singular_label,
        "plural_label": resp.plural_label,
        "fields": fields,
    })))
}
