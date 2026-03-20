mod client;
mod config;
mod convert;
mod error;
mod handlers;

pub mod proto {
    tonic::include_proto!("crap");
}

use std::net::{IpAddr, SocketAddr};

use axum::http::{HeaderValue, Method};
use clap::Parser;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::client::GrpcClient;
use crate::config::GatewayConfig;
use crate::handlers::{AppState, ProxyState};

#[derive(Parser)]
#[command(name = "crap-rest", about = "REST gateway for Crap CMS gRPC API")]
struct Cli {
    /// Listen port
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// gRPC server address
    #[arg(short, long, default_value = "http://localhost:50051")]
    grpc: String,

    /// Config file path (optional)
    #[arg(short, long)]
    config: Option<String>,

    /// Serve OpenAPI docs at /api (overrides config file)
    #[arg(long)]
    openapi: Option<bool>,

    /// Enable upload/file proxy to CMS HTTP server
    #[arg(long)]
    proxy: bool,

    /// CMS HTTP address for proxy (default: http://localhost:3000)
    #[arg(long)]
    cms_url: Option<String>,

    /// Enable WebSocket /subscribe endpoint for real-time events
    #[arg(long)]
    subscribe: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    // Load config file if provided, then override with CLI flags
    let mut cfg = match &cli.config {
        Some(path) => GatewayConfig::from_file(path)?,
        None => GatewayConfig::default(),
    };

    // CLI flags override config file
    if cli.port != 8080 {
        cfg.server.port = cli.port;
    }
    if cli.grpc != "http://localhost:50051" {
        cfg.grpc.address = cli.grpc;
    }
    if let Some(openapi) = cli.openapi {
        cfg.openapi.enabled = openapi;
    }
    if cli.proxy {
        cfg.proxy.enabled = true;
    }
    if let Some(url) = cli.cms_url {
        cfg.proxy.cms_url = url;
        cfg.proxy.enabled = true;
    }
    if cli.subscribe {
        cfg.subscribe.enabled = true;
    }

    cfg.validate()?;

    let client = GrpcClient::new(&cfg.grpc.address)?;

    let proxy = if cfg.proxy.enabled {
        tracing::info!("upload proxy enabled → {}", cfg.proxy.cms_url);
        Some(ProxyState {
            client: reqwest::Client::new(),
            cms_url: cfg.proxy.cms_url.clone(),
        })
    } else {
        tracing::info!("upload proxy disabled");
        None
    };

    let subscribe = if cfg.subscribe.enabled {
        tracing::info!(
            "WebSocket /subscribe enabled (ping={:?}, timeout={:?}, max_msg={}B)",
            cfg.subscribe.ping_interval,
            cfg.subscribe.timeout,
            cfg.subscribe.max_message_size,
        );
        Some(cfg.subscribe)
    } else {
        tracing::info!("WebSocket /subscribe disabled");
        None
    };

    let state = AppState {
        grpc: client,
        proxy,
        subscribe,
    };

    let allowed_methods = [Method::GET, Method::POST, Method::PATCH, Method::DELETE];
    let cors = if cfg.cors.allowed_origins.iter().any(|o| o == "*") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(allowed_methods)
            .allow_headers(Any)
    } else {
        let origins: Vec<HeaderValue> = cfg
            .cors
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(allowed_methods)
            .allow_headers(Any)
    };

    let app = handlers::router(state, &cfg.openapi)
        .layer(cors)
        .layer(CompressionLayer::new());

    let host: IpAddr = cfg
        .server
        .host
        .parse()
        .unwrap_or(std::net::Ipv6Addr::UNSPECIFIED.into());
    let addr = SocketAddr::from((host, cfg.server.port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("crap-rest listening on {}", addr);
    tracing::info!("gRPC target: {}", cfg.grpc.address);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
