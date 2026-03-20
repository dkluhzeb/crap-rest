use std::time::Duration;

use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws},
    http::HeaderMap,
    response::Response,
    routing::any,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Map, json};
use tokio::time::{Instant, interval};
use tonic::Request;
use tracing::{debug, info, warn};

use crate::client::GrpcClient;
use crate::config::SubscribeConfig;
use crate::convert::struct_to_json;
use crate::proto;

use super::AppState;

/// Disconnect if no Pong received within 2 ping intervals.
const PONG_GRACE_MULTIPLIER: u32 = 2;

pub fn routes() -> Router<AppState> {
    Router::new().route("/subscribe", any(ws_handler))
}

async fn ws_handler(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let header_token = extract_bearer_token(&headers);
    // subscribe is Some when routes are merged — unwrap_or for safety
    let cfg = state.subscribe.unwrap_or_default();
    let client = state.grpc;
    upgrade
        .max_message_size(cfg.max_message_size)
        .on_upgrade(move |socket| handle_socket(socket, client, header_token, cfg))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
}

#[derive(Debug, Default, Deserialize)]
struct SubscribeParams {
    #[serde(default)]
    collections: Vec<String>,
    #[serde(default)]
    globals: Vec<String>,
    #[serde(default)]
    operations: Vec<String>,
    token: Option<String>,
}

async fn handle_socket(
    mut socket: ws::WebSocket,
    client: GrpcClient,
    header_token: Option<String>,
    cfg: SubscribeConfig,
) {
    info!("WebSocket client connected");

    let timeout = cfg.timeout;
    let params = match recv_subscribe_params(&mut socket, timeout).await {
        Ok(p) => p,
        Err(e) => {
            warn!("subscribe handshake failed: {e}");
            let _ = send_error(&mut socket, &e).await;
            let _ = socket.close().await;
            return;
        }
    };

    debug!(
        collections = ?params.collections,
        globals = ?params.globals,
        operations = ?params.operations,
        "subscribe params received"
    );

    // Message token takes priority, header token is fallback
    let token = params.token.or(header_token).unwrap_or_default();

    let req = Request::new(proto::SubscribeRequest {
        collections: params.collections,
        globals: params.globals,
        operations: params.operations,
        token,
    });

    let stream = match client.client().subscribe(req).await {
        Ok(resp) => resp.into_inner(),
        Err(status) => {
            warn!("gRPC subscribe failed: {}", status.message());
            let _ = send_error(&mut socket, status.message()).await;
            let _ = socket.close().await;
            return;
        }
    };

    info!("gRPC subscribe stream established");
    forward_events(socket, stream, cfg.ping_interval).await;
    info!("WebSocket client disconnected");
}

async fn recv_subscribe_params(
    socket: &mut ws::WebSocket,
    timeout: Duration,
) -> Result<SubscribeParams, String> {
    let msg = tokio::time::timeout(timeout, socket.recv())
        .await
        .map_err(|_| "timeout waiting for subscribe message".to_string())?
        .ok_or_else(|| "connection closed before subscribe message".to_string())?
        .map_err(|e| format!("WebSocket error: {e}"))?;

    match msg {
        ws::Message::Text(text) => {
            serde_json::from_str(&text).map_err(|e| format!("invalid subscribe JSON: {e}"))
        }
        ws::Message::Close(_) => Err("connection closed".to_string()),
        _ => Err("expected JSON text message".to_string()),
    }
}

async fn forward_events(
    socket: ws::WebSocket,
    mut grpc_stream: tonic::Streaming<proto::MutationEvent>,
    ping_interval: Duration,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut ping_timer = interval(ping_interval);
    let pong_deadline = ping_interval.saturating_mul(PONG_GRACE_MULTIPLIER);
    let mut last_pong = Instant::now();

    // Skip the first immediate tick
    ping_timer.tick().await;

    loop {
        tokio::select! {
            event = grpc_stream.next() => {
                match event {
                    Some(Ok(mutation)) => {
                        debug!(
                            seq = mutation.sequence,
                            op = mutation.operation,
                            collection = mutation.collection,
                            "forwarding event"
                        );
                        let json_str = event_to_json(&mutation).to_string();
                        if sender.send(ws::Message::text(json_str)).await.is_err() {
                            debug!("send failed, client gone");
                            break;
                        }
                    }
                    Some(Err(status)) => {
                        warn!("gRPC stream error: {}", status.message());
                        let err = json!({ "error": status.message() }).to_string();
                        let _ = sender.send(ws::Message::text(err)).await;
                        break;
                    }
                    None => {
                        debug!("gRPC stream ended");
                        break;
                    }
                }
            }
            _ = ping_timer.tick() => {
                if last_pong.elapsed() > pong_deadline {
                    warn!("no pong received within {}s, closing", pong_deadline.as_secs());
                    break;
                }
                if sender.send(ws::Message::Ping(vec![].into())).await.is_err() {
                    debug!("ping failed, client gone");
                    break;
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(ws::Message::Pong(_))) => {
                        last_pong = Instant::now();
                    }
                    Some(Ok(ws::Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ignore other client messages
                }
            }
        }
    }

    let _ = sender.close().await;
}

fn event_to_json(event: &proto::MutationEvent) -> serde_json::Value {
    let mut map = Map::new();
    map.insert("sequence".into(), json!(event.sequence));
    map.insert("timestamp".into(), json!(event.timestamp));
    map.insert("target".into(), json!(event.target));
    map.insert("operation".into(), json!(event.operation));
    map.insert("collection".into(), json!(event.collection));
    map.insert("document_id".into(), json!(event.document_id));

    if let Some(ref data) = event.data {
        map.insert("data".into(), struct_to_json(data));
    } else {
        map.insert("data".into(), serde_json::Value::Null);
    }

    serde_json::Value::Object(map)
}

async fn send_error(socket: &mut ws::WebSocket, message: &str) -> Result<(), axum::Error> {
    let err = json!({ "error": message }).to_string();
    socket.send(ws::Message::text(err)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use prost_types::Struct;

    // --- extract_bearer_token ---

    #[test]
    fn bearer_token_extracted() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer abc123"));
        assert_eq!(extract_bearer_token(&headers), Some("abc123".to_string()));
    }

    #[test]
    fn bearer_token_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn bearer_token_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Basic abc123"));
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn bearer_token_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer "));
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn bearer_token_preserves_full_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.test"),
        );
        assert_eq!(
            extract_bearer_token(&headers),
            Some("eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.test".to_string())
        );
    }

    // --- SubscribeParams deserialization ---

    #[test]
    fn subscribe_params_full() {
        let json =
            r#"{"collections":["posts"],"globals":["nav"],"operations":["create"],"token":"xyz"}"#;
        let params: SubscribeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.collections, vec!["posts"]);
        assert_eq!(params.globals, vec!["nav"]);
        assert_eq!(params.operations, vec!["create"]);
        assert_eq!(params.token, Some("xyz".to_string()));
    }

    #[test]
    fn subscribe_params_empty_object() {
        let params: SubscribeParams = serde_json::from_str("{}").unwrap();
        assert!(params.collections.is_empty());
        assert!(params.globals.is_empty());
        assert!(params.operations.is_empty());
        assert!(params.token.is_none());
    }

    #[test]
    fn subscribe_params_partial() {
        let json = r#"{"collections":["posts","tags"]}"#;
        let params: SubscribeParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.collections, vec!["posts", "tags"]);
        assert!(params.operations.is_empty());
        assert!(params.token.is_none());
    }

    // --- event_to_json ---

    #[test]
    fn event_to_json_without_data() {
        let event = proto::MutationEvent {
            sequence: 1,
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            target: "collection".to_string(),
            operation: "create".to_string(),
            collection: "posts".to_string(),
            document_id: "abc123".to_string(),
            data: None,
        };

        let json = event_to_json(&event);
        assert_eq!(json["sequence"], 1);
        assert_eq!(json["timestamp"], "2024-01-15T10:30:00Z");
        assert_eq!(json["target"], "collection");
        assert_eq!(json["operation"], "create");
        assert_eq!(json["collection"], "posts");
        assert_eq!(json["document_id"], "abc123");
        assert!(json["data"].is_null());
    }

    #[test]
    fn event_to_json_with_data() {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "title".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue("Hello".to_string())),
            },
        );

        let event = proto::MutationEvent {
            sequence: 5,
            timestamp: "2024-01-15T11:00:00Z".to_string(),
            target: "collection".to_string(),
            operation: "update".to_string(),
            collection: "posts".to_string(),
            document_id: "def456".to_string(),
            data: Some(Struct { fields }),
        };

        let json = event_to_json(&event);
        assert_eq!(json["sequence"], 5);
        assert_eq!(json["data"]["title"], "Hello");
    }

    #[test]
    fn event_to_json_all_fields_present() {
        let event = proto::MutationEvent {
            sequence: 0,
            timestamp: String::new(),
            target: String::new(),
            operation: String::new(),
            collection: String::new(),
            document_id: String::new(),
            data: None,
        };

        let json = event_to_json(&event);
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("sequence"));
        assert!(obj.contains_key("timestamp"));
        assert!(obj.contains_key("target"));
        assert!(obj.contains_key("operation"));
        assert!(obj.contains_key("collection"));
        assert!(obj.contains_key("document_id"));
        assert!(obj.contains_key("data"));
        assert_eq!(obj.len(), 7);
    }
}
