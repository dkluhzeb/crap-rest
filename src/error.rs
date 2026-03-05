use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Wraps a tonic::Status and converts it to an appropriate HTTP response.
pub struct GrpcError(pub tonic::Status);

impl From<tonic::Status> for GrpcError {
    fn from(status: tonic::Status) -> Self {
        GrpcError(status)
    }
}

impl IntoResponse for GrpcError {
    fn into_response(self) -> Response {
        let grpc_code = self.0.code();
        let message = self.0.message().to_string();

        let http_status = match grpc_code {
            tonic::Code::NotFound => StatusCode::NOT_FOUND,
            tonic::Code::InvalidArgument => StatusCode::BAD_REQUEST,
            tonic::Code::PermissionDenied => StatusCode::FORBIDDEN,
            tonic::Code::Unauthenticated => StatusCode::UNAUTHORIZED,
            tonic::Code::AlreadyExists => StatusCode::CONFLICT,
            tonic::Code::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            tonic::Code::FailedPrecondition => StatusCode::BAD_REQUEST,
            tonic::Code::OutOfRange => StatusCode::BAD_REQUEST,
            tonic::Code::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (http_status, Json(json!({ "error": message }))).into_response()
    }
}

/// Result type alias for handler functions.
pub type RestResult<T> = Result<T, GrpcError>;
