//! Oracle HCM (People HQ) Model Context Protocol implementation.
//!
//! This crate provides MCP tools for interacting with Oracle HCM's REST API.
//! The implementation is organized into:
//!
//! - `error`: Error types and conversions
//! - `http`: HTTP client and API communication
//! - `tools`: Individual MCP tools for specific HCM operations
//!
//! The main entry point is the `OracleHCMMCPFactory` which provides the MCP server
//! implementation and manages all tools.

pub mod error;
pub mod http;
pub mod tools;

use anyhow::{Result, anyhow};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
    },
    model::{
        InitializeRequestParam, InitializeResult,
        ProtocolVersion, ServerCapabilities, ServerInfo, Implementation,
        GetPromptRequestParam, GetPromptResult, ListPromptsResult,
        PaginatedRequestParam,
    },
    service::RequestContext,
    prompt_handler, tool_handler, prompt_router, tool_router,
};
use axum::http::request;
use tracing::info;

// Use re-exported items from http module
use crate::mcp::http::{
    HCM_BASE_URL,
    HCM_API_VERSION,
    REST_FRAMEWORK_VERSION,
    HCM_USERNAME,
    HCM_PASSWORD,
};

#[derive(Clone)]
pub struct OracleHCMMCPFactory {
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

#[tool_router]
impl OracleHCMMCPFactory {
    pub fn new() -> Result<Self> {
        // Eagerly evaluate LazyLock and propagate errors during initialization
        let _ = HCM_BASE_URL.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_BASE_URL: {e}"))?;
        let _ = HCM_API_VERSION.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_API_VERSION: {e}"))?;
        let _ = REST_FRAMEWORK_VERSION.as_ref()
            .map_err(|e| anyhow!("Failed to load REST_FRAMEWORK_VERSION: {e}"))?;
        let _ = HCM_USERNAME.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_USERNAME: {e}"))?;
        let _ = HCM_PASSWORD.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_PASSWORD: {e}"))?;

        Ok(Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        })
    }
}

#[prompt_router]
impl OracleHCMMCPFactory {}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for OracleHCMMCPFactory {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Oracle HCM (also known as People HQ at Westpac) MCP Server with tools prompts and resources"
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        if let Some(http_request_part) = context.extensions.get::<request::Parts>() {
            let initialize_headers = &http_request_part.headers;
            let initialize_uri = &http_request_part.uri;
            info!(?initialize_headers, %initialize_uri, "initialize from http server");
        }
        Ok(self.get_info())
    }
}
