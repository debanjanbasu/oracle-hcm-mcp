use anyhow::Result;
use axum::{Router, serve};
use dotenv::dotenv;
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};
use tokio::{net::TcpListener, signal};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod mcp;
use mcp::hcm::OracleHCMMCPFactory;

const BIND_ADDRESS: &str = "0.0.0.0:8080";

#[tokio::main]
async fn main() -> Result<()> {
    // Load variables from .env file if it exists into the environment
    dotenv().ok();

    // Initialize tracing
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
        || Ok(OracleHCMMCPFactory::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    // Starting the server... Setting up the router and TCP listener
    info!("Starting server on {}", BIND_ADDRESS);
    let router = Router::new().nest_service("/mcp", service);
    let tcp_listener = TcpListener::bind(BIND_ADDRESS).await?;

    // Graceful shutdown on CTRL+C
    let shutdown = async {
        signal::ctrl_c().await.unwrap_or_else(|e| {
            eprintln!("failed to install CTRL+C handler: {e}");
        });
    };

    // Finally start the server with graceful shutdown
    serve(tcp_listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}
