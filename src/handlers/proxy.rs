use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{delete, get, patch, post},
};

use super::AppState;

/// Reject path segments that could be used for path traversal.
fn reject_traversal(segments: &[&str]) -> Result<(), StatusCode> {
    for s in segments {
        if s.contains("..") || s.contains('/') || s.contains('\\') {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/uploads/{slug}", post(create_upload))
        .route("/uploads/{slug}/{id}", patch(update_upload))
        .route("/uploads/{slug}/{id}", delete(delete_upload))
        .route("/uploads/{slug}/{filename}", get(serve_file))
}

async fn create_upload(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, StatusCode> {
    reject_traversal(&[&slug])?;
    let proxy = state.proxy.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let url = format!("{}/api/upload/{}", proxy.cms_url, slug);

    let mut req = proxy.client.post(&url);
    req = forward_header(req, &headers, header::AUTHORIZATION);
    req = forward_header(req, &headers, header::CONTENT_TYPE);
    req = req.body(reqwest::Body::wrap_stream(body.into_data_stream()));

    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    proxy_response(resp).await
}

async fn update_upload(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, StatusCode> {
    reject_traversal(&[&slug, &id])?;
    let proxy = state.proxy.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let url = format!("{}/api/upload/{}/{}", proxy.cms_url, slug, id);

    let mut req = proxy.client.patch(&url);
    req = forward_header(req, &headers, header::AUTHORIZATION);
    req = forward_header(req, &headers, header::CONTENT_TYPE);
    req = req.body(reqwest::Body::wrap_stream(body.into_data_stream()));

    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    proxy_response(resp).await
}

async fn delete_upload(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    reject_traversal(&[&slug, &id])?;
    let proxy = state.proxy.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let url = format!("{}/api/upload/{}/{}", proxy.cms_url, slug, id);

    let mut req = proxy.client.delete(&url);
    req = forward_header(req, &headers, header::AUTHORIZATION);

    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    proxy_response(resp).await
}

async fn serve_file(
    State(state): State<AppState>,
    Path((slug, filename)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    reject_traversal(&[&slug, &filename])?;
    let proxy = state.proxy.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let url = format!("{}/uploads/{}/{}", proxy.cms_url, slug, filename);

    let mut req = proxy.client.get(&url);
    for key in [
        header::AUTHORIZATION,
        header::ACCEPT,
        header::RANGE,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
    ] {
        req = forward_header(req, &headers, key);
    }

    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    proxy_response(resp).await
}

fn forward_header(
    req: reqwest::RequestBuilder,
    headers: &HeaderMap,
    key: header::HeaderName,
) -> reqwest::RequestBuilder {
    if let Some(val) = headers.get(&key) {
        req.header(key, val.as_bytes())
    } else {
        req
    }
}

const PASS_HEADERS: [header::HeaderName; 9] = [
    header::CONTENT_TYPE,
    header::CONTENT_LENGTH,
    header::CACHE_CONTROL,
    header::CONTENT_DISPOSITION,
    header::ETAG,
    header::LAST_MODIFIED,
    header::VARY,
    header::CONTENT_RANGE,
    header::ACCEPT_RANGES,
];

async fn proxy_response(resp: reqwest::Response) -> Result<Response, StatusCode> {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut builder = Response::builder().status(status);
    for key in &PASS_HEADERS {
        if let Some(val) = resp.headers().get(key) {
            builder = builder.header(key, val);
        }
    }

    builder
        .body(Body::from_stream(resp.bytes_stream()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_traversal_allows_normal_segments() {
        assert!(reject_traversal(&["posts"]).is_ok());
        assert!(reject_traversal(&["my-collection", "abc123"]).is_ok());
        assert!(reject_traversal(&["uploads", "image.png"]).is_ok());
    }

    #[test]
    fn reject_traversal_blocks_dot_dot() {
        assert_eq!(reject_traversal(&[".."]), Err(StatusCode::BAD_REQUEST));
        assert_eq!(reject_traversal(&["..%2f"]), Err(StatusCode::BAD_REQUEST));
        assert_eq!(
            reject_traversal(&["foo..bar"]),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn reject_traversal_blocks_forward_slash() {
        assert_eq!(reject_traversal(&["foo/bar"]), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn reject_traversal_blocks_backslash() {
        assert_eq!(
            reject_traversal(&["foo\\bar"]),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn reject_traversal_checks_all_segments() {
        // First segment ok, second has traversal
        assert_eq!(
            reject_traversal(&["valid", "../etc/passwd"]),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn reject_traversal_allows_single_dot() {
        // A single dot is not ".." — should be allowed
        assert!(reject_traversal(&["file.txt"]).is_ok());
        assert!(reject_traversal(&[".hidden"]).is_ok());
    }

    #[test]
    fn reject_traversal_empty_segments() {
        assert!(reject_traversal(&[]).is_ok());
        assert!(reject_traversal(&[""]).is_ok());
    }
}
