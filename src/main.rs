use anyhow::Result;
use axum::{Router, serve};
use dotenv::dotenv;
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use std::io::Error as IoError;
use tokio::{net::TcpListener, signal::ctrl_c};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod mcp;
use mcp::OracleHCMMCPFactory;

const BIND_ADDRESS: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file for configuration
    // Continues even if .env doesn't exist (ok() handles the Result)
    // Then it would load the variables from the actual environment
    dotenv().ok();

    // Initialize structured logging with tracing
    // Uses RUST_LOG env var if set, defaults to "debug" level
    // Format: error/warn/info/debug/trace
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".to_string().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Setting up the Streamable HTTP Service
    info!("Setting up the Streamable HTTP Service from various MCP Factories");
    let service = StreamableHttpService::new(
        || {
            OracleHCMMCPFactory::new()
                .map_err(IoError::other)
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    // Set up HTTP server components:
    // 1. Router with /mcp endpoint for our MCP service
    // 2. TCP listener on configured address
    info!("Starting server on {}", BIND_ADDRESS);
    let router = Router::new().nest_service("/mcp", service);
    let tcp_listener = TcpListener::bind(BIND_ADDRESS).await?;

    // Start server with graceful shutdown handling
    // Waits for all in-flight requests to complete on CTRL+C
    serve(tcp_listener, router)
        .with_graceful_shutdown(async {
            ctrl_c().await.unwrap_or_else(|e| {
                eprintln!("failed to install CTRL+C handler: {e}");
            });
        })
        .await?;

    Ok(())
}
