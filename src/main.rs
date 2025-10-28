//! Oracle HCM MCP Server
//!
//! A high-performance Model Context Protocol (MCP) server for Oracle HCM (Human Capital Management).
//! Provides AI assistants with tools to query employee data, absence balances, and leave information.
//!
//! # Architecture
//! - **Transport**: Streamable HTTP (SSE-based) for real-time communication
//! - **Protocol**: MCP 1.0 with JSON-RPC 2.0
//! - **API**: Oracle HCM REST API with rustls-native TLS
//! - **Observability**: Structured logging with tracing
//!
//! # Configuration
//! All configuration is via environment variables (see `.env.example`):
//! - `HCM_BASE_URL`: Your Oracle HCM instance URL (required)
//! - `HCM_PASSWORD`: API password (required)
//! - `HCM_USERNAME`: API username (optional, defaults to "`WBC_HR_AGENT`")
//! - `HCM_API_VERSION`: API version (optional, defaults to "11.13.18.05")
//! - `REST_FRAMEWORK_VERSION`: Framework version (optional, defaults to "9")
//! - `RUST_LOG`: Logging level (optional, defaults to "info")
//!
//! # Server Endpoints
//! - `POST /mcp`: MCP protocol endpoint (JSON-RPC over SSE)
//!
//! # Graceful Shutdown
//! The server handles SIGINT (Ctrl+C) gracefully by:
//! 1. Stopping acceptance of new connections
//! 2. Waiting for in-flight requests to complete
//! 3. Cleaning up resources before exit

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

/// Server bind address - listens on all interfaces, port 8080
const BIND_ADDRESS: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file (optional, for local development)
    // In production, env vars should be set by the container runtime
    dotenv().ok();

    // Initialize structured logging
    // Supports RUST_LOG env var for filtering (e.g., RUST_LOG=debug,oracle_hcm_mcp=trace)
    // Default level is "info" for production use
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Oracle HCM MCP Server starting...");
    info!("Bind address: {}", BIND_ADDRESS);

    // Create the MCP service with Oracle HCM tools
    // Uses local session management (in-memory, suitable for single-instance deployment)
    let service = StreamableHttpService::new(
        || {
            // Factory function called for each new MCP session
            // Validates configuration early and fails fast if env vars are missing
            OracleHCMMCPFactory::new()
                .map_err(IoError::other)
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    // Build HTTP router with the MCP endpoint
    // All MCP communication happens through POST /mcp
    let router = Router::new().nest_service("/mcp", service);
    
    // Bind to the configured address
    let tcp_listener = TcpListener::bind(BIND_ADDRESS).await?;
    info!("Server listening on {}", BIND_ADDRESS);
    info!("MCP endpoint available at http://{}/mcp", BIND_ADDRESS);

    // Start the server with graceful shutdown on Ctrl+C
    // This ensures all in-flight requests complete before shutdown
    serve(tcp_listener, router)
        .with_graceful_shutdown(async {
            let _ = ctrl_c().await;
            info!("Received shutdown signal, draining connections...");
        })
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}
