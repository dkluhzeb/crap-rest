use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::convert::document_to_json;
use crate::error::RestResult;
use crate::proto;

use super::{AppState, make_request};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/{collection}/login", post(login))
        .route("/auth/me", get(me))
        .route("/auth/{collection}/forgot-password", post(forgot_password))
        .route("/auth/{collection}/reset-password", post(reset_password))
        .route("/auth/{collection}/verify-email", post(verify_email))
        .route("/auth/{collection}/{id}/lock", post(lock_account))
        .route("/auth/{collection}/{id}/unlock", post(unlock_account))
        .route("/auth/{collection}/{id}/verify", post(verify_account))
        .route("/auth/{collection}/{id}/unverify", post(unverify_account))
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

async fn lock_account(
    State(client): State<GrpcClient>,
    Path((collection, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::AccountActionRequest { collection, id });

    let resp = client.client().lock_account(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

async fn unlock_account(
    State(client): State<GrpcClient>,
    Path((collection, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::AccountActionRequest { collection, id });

    let resp = client.client().unlock_account(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

async fn verify_account(
    State(client): State<GrpcClient>,
    Path((collection, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::AccountActionRequest { collection, id });

    let resp = client.client().verify_account(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}

async fn unverify_account(
    State(client): State<GrpcClient>,
    Path((collection, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::AccountActionRequest { collection, id });

    let resp = client.client().unverify_account(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "success": resp.success })))
}
