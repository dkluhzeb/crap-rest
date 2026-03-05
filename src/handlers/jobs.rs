use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::client::GrpcClient;
use crate::error::RestResult;
use crate::proto;

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
        .route("/api/jobs", get(list_jobs))
        .route("/api/jobs/{slug}/trigger", post(trigger_job))
        .route("/api/jobs/runs/{id}", get(get_job_run))
        .route("/api/jobs/runs", get(list_job_runs))
}

async fn list_jobs(
    State(client): State<GrpcClient>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::ListJobsRequest {});
    let resp = client.client().list_jobs(req).await?.into_inner();

    let jobs: Vec<Value> = resp
        .jobs
        .iter()
        .map(|j| {
            serde_json::json!({
                "slug": j.slug,
                "handler": j.handler,
                "schedule": j.schedule,
                "queue": j.queue,
                "retries": j.retries,
                "timeout": j.timeout,
                "concurrency": j.concurrency,
                "skip_if_running": j.skip_if_running,
                "label": j.label,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

#[derive(Deserialize, Default)]
struct TriggerBody {
    data: Option<Value>,
}

async fn trigger_job(
    State(client): State<GrpcClient>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<TriggerBody>,
) -> RestResult<Json<Value>> {
    let data_json = body.data.map(|v| v.to_string());
    let req = make_request(
        &headers,
        proto::TriggerJobRequest { slug, data_json },
    );

    let resp = client.client().trigger_job(req).await?.into_inner();
    Ok(Json(serde_json::json!({ "job_id": resp.job_id })))
}

async fn get_job_run(
    State(client): State<GrpcClient>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(&headers, proto::GetJobRunRequest { id });
    let resp = client.client().get_job_run(req).await?.into_inner();
    Ok(Json(job_run_to_json(&resp)))
}

#[derive(Debug, Deserialize, Default)]
pub struct JobRunParams {
    pub slug: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list_job_runs(
    State(client): State<GrpcClient>,
    Query(params): Query<JobRunParams>,
    headers: HeaderMap,
) -> RestResult<Json<Value>> {
    let req = make_request(
        &headers,
        proto::ListJobRunsRequest {
            slug: params.slug,
            status: params.status,
            limit: params.limit,
            offset: params.offset,
        },
    );

    let resp = client.client().list_job_runs(req).await?.into_inner();
    let runs: Vec<Value> = resp.runs.iter().map(job_run_to_json).collect();
    Ok(Json(serde_json::json!({ "runs": runs })))
}

fn job_run_to_json(r: &proto::GetJobRunResponse) -> Value {
    serde_json::json!({
        "id": r.id,
        "slug": r.slug,
        "status": r.status,
        "data_json": r.data_json,
        "result_json": r.result_json,
        "error": r.error,
        "attempt": r.attempt,
        "max_attempts": r.max_attempts,
        "scheduled_by": r.scheduled_by,
        "created_at": r.created_at,
        "started_at": r.started_at,
        "completed_at": r.completed_at,
    })
}
