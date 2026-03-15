use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::document_to_json;
use crate::error::RestResult;
use crate::proto;

use super::make_request;

pub fn routes() -> Router<GrpcClient> {
    Router::new()
        .route("/api/auth/{collection}/login", post(login))
        .route("/api/auth/me", get(me))
        .route(
            "/api/auth/{collection}/forgot-password",
            post(forgot_password),
        )
        .route(
            "/api/auth/{collection}/reset-password",
            post(reset_password),
        )
        .route("/api/auth/{collection}/verify-email", post(verify_email))
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

async fn login(
    State(client): State<GrpcClient>,
    Path(collection): Path<String>,
    Json(body): Json<LoginBody>,
) -> RestResult<Json<Value>> {
    let req = tonic::Request::new(proto::LoginRequest {
        collection,
        email: body.email,
        password: body.password,
    });

    let resp = client.client().login(req).await?.into_inner();
    let user = resp.user.map(|u| document_to_json(&u));

    Ok(Json(serde_json::json!({
        "token": resp.token,
        "user": user,
    })))
}

async fn me(State(client): State<GrpcClient>, headers: HeaderMap) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::MeRequest::default());
    let resp = client.client().me(req).await?.into_inner();

    match resp.user {
        Some(user) => Ok(Json(document_to_json(&user))),
        None => Err(tonic::Status::unauthenticated("not authenticated").into()),
    }
}

#[derive(Deserialize)]
struct ForgotPasswordBody {
    email: String,
}

async fn forgot_password(
    State(client): State<GrpcClient>,
    Path(collection): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ForgotPasswordBody>,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::ForgotPasswordRequest {
            collection,
            email: body.email,
        },
    );

    let resp = client.client().forgot_password(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

#[derive(Deserialize)]
struct ResetPasswordBody {
    token: String,
    new_password: String,
}

async fn reset_password(
    State(client): State<GrpcClient>,
    Path(collection): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ResetPasswordBody>,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::ResetPasswordRequest {
            collection,
            token: body.token,
            new_password: body.new_password,
        },
    );

    let resp = client.client().reset_password(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

#[derive(Deserialize)]
struct VerifyEmailBody {
    token: String,
}

async fn verify_email(
    State(client): State<GrpcClient>,
    Path(collection): Path<String>,
    headers: HeaderMap,
    Json(body): Json<VerifyEmailBody>,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::VerifyEmailRequest {
            collection,
            token: body.token,
        },
    );

    let resp = client.client().verify_email(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}
