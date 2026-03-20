use std::sync::Arc;

use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde_json::{Map, Value, json};
use tokio::sync::OnceCell;
use tracing::warn;

use crate::client::GrpcClient;
use crate::config::OpenApiConfig;
use crate::proto;

#[derive(Clone)]
pub struct OpenApiState {
    pub client: GrpcClient,
    pub config: Arc<OpenApiConfig>,
    pub cached_spec: Arc<OnceCell<String>>,
}

pub fn routes(client: GrpcClient, config: &OpenApiConfig) -> Router {
    if !config.enabled {
        return Router::new();
    }
    let state = OpenApiState {
        client,
        config: Arc::new(config.clone()),
        cached_spec: Arc::new(OnceCell::new()),
    };
    Router::new()
        .route("/", get(scalar_ui))
        .route("/openapi.json", get(openapi_json))
        .with_state(state)
}

async fn scalar_ui() -> impl IntoResponse {
    let html = r#"<!DOCTYPE html>
<html>
<head>
  <title>API Reference</title>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
</head>
<body>
  <script id="api-reference" data-url="./openapi.json"></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>"#;

    (StatusCode::OK, [(header::CONTENT_TYPE, "text/html")], html)
}

async fn openapi_json(State(state): State<OpenApiState>) -> impl IntoResponse {
    let result = state
        .cached_spec
        .get_or_try_init(|| async {
            let spec = generate_spec(&state.client, &state.config).await?;
            Ok::<_, anyhow::Error>(spec.to_string())
        })
        .await;

    match result {
        Ok(spec) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            spec.clone(),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "application/json")],
            json!({"error": format!("Failed to generate spec: {e}")}).to_string(),
        ),
    }
}

async fn generate_spec(client: &GrpcClient, config: &OpenApiConfig) -> anyhow::Result<Value> {
    // List all collections and globals
    let list_resp = client
        .client()
        .list_collections(tonic::Request::new(proto::ListCollectionsRequest {}))
        .await?
        .into_inner();

    // Describe all collections and globals in parallel
    let collection_futs: Vec<_> = list_resp
        .collections
        .iter()
        .map(|c| {
            let mut cl = client.client();
            let slug = c.slug.clone();
            async move {
                let resp = cl
                    .describe_collection(tonic::Request::new(proto::DescribeCollectionRequest {
                        slug,
                        is_global: false,
                    }))
                    .await?;
                Ok::<_, tonic::Status>(resp.into_inner())
            }
        })
        .collect();

    let global_futs: Vec<_> = list_resp
        .globals
        .iter()
        .map(|g| {
            let mut cl = client.client();
            let slug = g.slug.clone();
            async move {
                let resp = cl
                    .describe_collection(tonic::Request::new(proto::DescribeCollectionRequest {
                        slug,
                        is_global: true,
                    }))
                    .await?;
                Ok::<_, tonic::Status>(resp.into_inner())
            }
        })
        .collect();

    let collection_descs = futures::future::join_all(collection_futs).await;
    let global_descs = futures::future::join_all(global_futs).await;

    let mut paths = Map::new();
    let mut schemas = Map::new();
    let has_auth = list_resp.collections.iter().any(|c| c.auth);

    // Build collection paths and schemas
    for (info, desc_result) in list_resp.collections.iter().zip(collection_descs) {
        let desc = match desc_result {
            Ok(d) => d,
            Err(e) => {
                warn!("failed to describe collection '{}': {e}", info.slug);
                continue;
            }
        };

        let slug = &info.slug;
        let label = info.singular_label.as_deref().unwrap_or(slug);
        let plural = info.plural_label.as_deref().unwrap_or(slug);

        // Build schema for this collection
        let schema_name = capitalize(slug);
        let schema = build_document_schema(&desc.fields, desc.timestamps);
        schemas.insert(schema_name.clone(), schema);

        // Build input schema (no id, no timestamps)
        let input_name = format!("{schema_name}Input");
        let input_schema = build_input_schema(&desc.fields);
        schemas.insert(input_name.clone(), input_schema);

        let schema_ref = json!({"$ref": format!("#/components/schemas/{schema_name}")});
        let input_ref = json!({"$ref": format!("#/components/schemas/{input_name}")});

        // GET /collections/{slug}
        let find_path = format!("/collections/{slug}");
        paths.insert(
            find_path.clone(),
            json!({
                "get": {
                    "summary": format!("Find {plural}"),
                    "operationId": format!("find_{slug}"),
                    "tags": [slug],
                    "parameters": find_query_params(),
                    "responses": {
                        "200": {
                            "description": "Success",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "docs": { "type": "array", "items": schema_ref },
                                            "pagination": { "$ref": "#/components/schemas/PaginationInfo" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "summary": format!("Create {label}"),
                    "operationId": format!("create_{slug}"),
                    "tags": [slug],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": { "schema": input_ref }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Created document",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        }
                    }
                }
            }),
        );

        // GET/PATCH/DELETE /collections/{slug}/{id}
        let id_path = format!("/collections/{slug}/{{id}}");
        paths.insert(
            id_path,
            json!({
                "get": {
                    "summary": format!("Get {label} by ID"),
                    "operationId": format!("get_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "depth", "in": "query", "schema": { "type": "integer" } },
                        { "name": "locale", "in": "query", "schema": { "type": "string" } },
                        { "name": "select", "in": "query", "schema": { "type": "string" }, "description": "Comma-separated field names" },
                        { "name": "draft", "in": "query", "schema": { "type": "boolean" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Document",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        },
                        "404": { "description": "Not found" }
                    }
                },
                "patch": {
                    "summary": format!("Update {label}"),
                    "operationId": format!("update_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": { "schema": input_ref }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated document",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        }
                    }
                },
                "delete": {
                    "summary": format!("Delete {label}"),
                    "operationId": format!("delete_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Deleted",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "success": { "type": "boolean" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        // GET /collections/{slug}/count
        let count_path = format!("/collections/{slug}/count");
        paths.insert(
            count_path,
            json!({
                "get": {
                    "summary": format!("Count {plural}"),
                    "operationId": format!("count_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "where", "in": "query", "schema": { "type": "string" }, "description": "JSON filter" },
                        { "name": "locale", "in": "query", "schema": { "type": "string" } },
                        { "name": "draft", "in": "query", "schema": { "type": "boolean" } },
                        { "name": "search", "in": "query", "schema": { "type": "string" }, "description": "FTS5 full-text search query" }
                    ],
                    "responses": {
                        "200": {
                            "description": "Count result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "count": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        // Bulk operations
        let bulk_path = format!("/collections/{slug}/bulk");
        paths.insert(
            bulk_path,
            json!({
                "patch": {
                    "summary": format!("Bulk update {plural}"),
                    "operationId": format!("update_many_{slug}"),
                    "tags": [slug],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "where": { "type": "string", "description": "JSON filter" },
                                        "data": input_ref,
                                        "locale": { "type": "string" },
                                        "draft": { "type": "boolean" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Bulk update result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "modified": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "summary": format!("Bulk delete {plural}"),
                    "operationId": format!("delete_many_{slug}"),
                    "tags": [slug],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "where": { "type": "string", "description": "JSON filter" }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Bulk delete result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "deleted": { "type": "integer" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        // Version paths
        let versions_path = format!("/collections/{slug}/{{id}}/versions");
        paths.insert(
            versions_path,
            json!({
                "get": {
                    "summary": format!("List {label} versions"),
                    "operationId": format!("list_versions_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "limit", "in": "query", "schema": { "type": "integer" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Version list",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "versions": {
                                                "type": "array",
                                                "items": { "$ref": "#/components/schemas/VersionInfo" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );

        let restore_path = format!("/collections/{slug}/{{id}}/versions/{{version_id}}/restore");
        paths.insert(
            restore_path,
            json!({
                "post": {
                    "summary": format!("Restore {label} version"),
                    "operationId": format!("restore_version_{slug}"),
                    "tags": [slug],
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "version_id", "in": "path", "required": true, "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Restored document",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        }
                    }
                }
            }),
        );

        // Auth paths for auth collections
        if info.auth {
            let auth_tag = format!("{slug} auth");

            paths.insert(
                format!("/auth/{slug}/login"),
                json!({
                    "post": {
                        "summary": format!("Login to {slug}"),
                        "operationId": format!("login_{slug}"),
                        "tags": [&auth_tag],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["email", "password"],
                                        "properties": {
                                            "email": { "type": "string", "format": "email" },
                                            "password": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "description": "Login success",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "token": { "type": "string" },
                                                "user": schema_ref
                                            }
                                        }
                                    }
                                }
                            },
                            "401": { "description": "Invalid credentials" }
                        }
                    }
                }),
            );

            paths.insert(
                format!("/auth/{slug}/forgot-password"),
                json!({
                    "post": {
                        "summary": "Request password reset",
                        "operationId": format!("forgot_password_{slug}"),
                        "tags": [&auth_tag],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["email"],
                                        "properties": {
                                            "email": { "type": "string", "format": "email" }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "description": "Success",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "success": { "type": "boolean" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }),
            );

            paths.insert(
                format!("/auth/{slug}/reset-password"),
                json!({
                    "post": {
                        "summary": "Reset password with token",
                        "operationId": format!("reset_password_{slug}"),
                        "tags": [&auth_tag],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["token", "new_password"],
                                        "properties": {
                                            "token": { "type": "string" },
                                            "new_password": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "description": "Success",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "success": { "type": "boolean" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }),
            );

            paths.insert(
                format!("/auth/{slug}/verify-email"),
                json!({
                    "post": {
                        "summary": "Verify email address",
                        "operationId": format!("verify_email_{slug}"),
                        "tags": [&auth_tag],
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["token"],
                                        "properties": {
                                            "token": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "description": "Success",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "success": { "type": "boolean" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }),
            );
        }
    }

    // Auth me endpoint (if any auth collection exists)
    if has_auth {
        paths.insert(
            "/auth/me".to_string(),
            json!({
                "get": {
                    "summary": "Get current user",
                    "operationId": "auth_me",
                    "tags": ["auth"],
                    "security": [{ "bearer": [] }],
                    "responses": {
                        "200": {
                            "description": "Current user",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object"
                                    }
                                }
                            }
                        },
                        "401": { "description": "Unauthenticated" }
                    }
                }
            }),
        );
    }

    // Build global paths
    for (info, desc_result) in list_resp.globals.iter().zip(global_descs) {
        let desc = match desc_result {
            Ok(d) => d,
            Err(e) => {
                warn!("failed to describe global '{}': {e}", info.slug);
                continue;
            }
        };

        let slug = &info.slug;
        let label = info.singular_label.as_deref().unwrap_or(slug);

        let schema_name = format!("Global{}", capitalize(slug));
        let schema = build_input_schema(&desc.fields);
        schemas.insert(schema_name.clone(), schema);

        let schema_ref = json!({"$ref": format!("#/components/schemas/{schema_name}")});

        let path = format!("/globals/{slug}");
        paths.insert(
            path,
            json!({
                "get": {
                    "summary": format!("Get {label} global"),
                    "operationId": format!("get_global_{slug}"),
                    "tags": ["globals"],
                    "parameters": [
                        { "name": "locale", "in": "query", "schema": { "type": "string" } }
                    ],
                    "responses": {
                        "200": {
                            "description": "Global document",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        }
                    }
                },
                "patch": {
                    "summary": format!("Update {label} global"),
                    "operationId": format!("update_global_{slug}"),
                    "tags": ["globals"],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": { "schema": schema_ref }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Updated global",
                            "content": {
                                "application/json": { "schema": schema_ref }
                            }
                        }
                    }
                }
            }),
        );
    }

    // Add shared schemas
    schemas.insert(
        "PaginationInfo".to_string(),
        json!({
            "type": "object",
            "properties": {
                "totalDocs": { "type": "integer", "description": "Total matching documents" },
                "limit": { "type": "integer", "description": "Applied page size" },
                "totalPages": { "type": "integer", "description": "Total pages" },
                "page": { "type": "integer", "description": "Current page (1-based)" },
                "pageStart": { "type": "integer", "description": "1-based index of first doc on current page" },
                "hasPrevPage": { "type": "boolean", "description": "Whether a previous page exists" },
                "hasNextPage": { "type": "boolean", "description": "Whether a next page exists" },
                "prevPage": { "type": "integer", "description": "Previous page number" },
                "nextPage": { "type": "integer", "description": "Next page number" },
                "startCursor": { "type": "string", "description": "Opaque cursor of first doc in results (cursor mode)" },
                "endCursor": { "type": "string", "description": "Opaque cursor of last doc in results (cursor mode)" }
            },
            "required": ["totalDocs", "limit", "hasPrevPage", "hasNextPage"]
        }),
    );
    schemas.insert(
        "VersionInfo".to_string(),
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "version": { "type": "integer" },
                "status": { "type": "string" },
                "latest": { "type": "boolean" },
                "created_at": { "type": "string" }
            }
        }),
    );

    // Build security schemes
    let mut security_schemes = Map::new();
    if has_auth {
        security_schemes.insert(
            "bearer".to_string(),
            json!({
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT"
            }),
        );
    }

    let spec = json!({
        "openapi": "3.1.0",
        "info": {
            "title": config.title,
            "version": config.version,
        },
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(schemas),
            "securitySchemes": Value::Object(security_schemes),
        }
    });

    Ok(spec)
}

fn find_query_params() -> Value {
    json!([
        { "name": "where", "in": "query", "schema": { "type": "string" }, "description": "JSON filter expression" },
        { "name": "order_by", "in": "query", "schema": { "type": "string" }, "description": "Sort expression" },
        { "name": "limit", "in": "query", "schema": { "type": "integer" } },
        { "name": "page", "in": "query", "schema": { "type": "integer" }, "description": "Page number (1-based)" },
        { "name": "depth", "in": "query", "schema": { "type": "integer" }, "description": "Relationship population depth" },
        { "name": "locale", "in": "query", "schema": { "type": "string" } },
        { "name": "select", "in": "query", "schema": { "type": "string" }, "description": "Comma-separated field names" },
        { "name": "draft", "in": "query", "schema": { "type": "boolean" } },
        { "name": "after_cursor", "in": "query", "schema": { "type": "string" }, "description": "Opaque forward cursor for keyset pagination" },
        { "name": "before_cursor", "in": "query", "schema": { "type": "string" }, "description": "Opaque backward cursor for keyset pagination" },
        { "name": "search", "in": "query", "schema": { "type": "string" }, "description": "FTS5 full-text search query" }
    ])
}

fn field_to_json_schema(f: &proto::FieldInfo) -> Value {
    match f.r#type.as_str() {
        "text" | "email" | "textarea" | "richtext" | "code" | "date" => {
            json!({"type": "string"})
        }
        "number" => json!({"type": "number"}),
        "checkbox" => json!({"type": "integer", "description": "0 or 1"}),
        "select" => {
            if f.options.is_empty() {
                json!({"type": "string"})
            } else {
                let values: Vec<&str> = f.options.iter().map(|o| o.value.as_str()).collect();
                json!({"type": "string", "enum": values})
            }
        }
        "relationship" | "upload" => {
            let has_many = f.relationship_has_many.unwrap_or(false);
            let collection = f.relationship_collection.as_deref().unwrap_or("unknown");
            if has_many {
                json!({
                    "type": "array",
                    "items": { "type": "string" },
                    "description": format!("IDs referencing {collection}")
                })
            } else {
                json!({
                    "type": "string",
                    "description": format!("ID referencing {collection}")
                })
            }
        }
        "group" => {
            let mut props = Map::new();
            for sub in &f.fields {
                props.insert(sub.name.clone(), field_to_json_schema(sub));
            }
            json!({"type": "object", "properties": Value::Object(props)})
        }
        "array" => {
            let mut props = Map::new();
            for sub in &f.fields {
                props.insert(sub.name.clone(), field_to_json_schema(sub));
            }
            json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": Value::Object(props)
                }
            })
        }
        "blocks" => {
            let one_of: Vec<Value> = f
                .blocks
                .iter()
                .map(|b| {
                    let mut props = Map::new();
                    props.insert(
                        "blockType".to_string(),
                        json!({"type": "string", "const": b.block_type}),
                    );
                    for sub in &b.fields {
                        props.insert(sub.name.clone(), field_to_json_schema(sub));
                    }
                    let label = b.label.as_deref().unwrap_or(&b.block_type);
                    json!({
                        "type": "object",
                        "title": label,
                        "required": ["blockType"],
                        "properties": Value::Object(props)
                    })
                })
                .collect();
            json!({"type": "array", "items": {"oneOf": one_of}})
        }
        "json" => json!({}), // any type
        _ => json!({"type": "string"}),
    }
}

fn build_document_schema(fields: &[proto::FieldInfo], timestamps: bool) -> Value {
    let mut props = Map::new();
    props.insert("id".to_string(), json!({"type": "string"}));

    for f in fields {
        props.insert(f.name.clone(), field_to_json_schema(f));
    }

    if timestamps {
        props.insert("created_at".to_string(), json!({"type": "string"}));
        props.insert("updated_at".to_string(), json!({"type": "string"}));
    }

    json!({"type": "object", "properties": Value::Object(props)})
}

fn build_input_schema(fields: &[proto::FieldInfo]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();

    for f in fields {
        props.insert(f.name.clone(), field_to_json_schema(f));
        if f.required {
            required.push(Value::String(f.name.clone()));
        }
    }

    let mut schema = json!({"type": "object", "properties": Value::Object(props)});
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
