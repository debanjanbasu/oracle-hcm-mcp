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
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{
        InitializeRequestParam, InitializeResult, CallToolResult,
        ProtocolVersion, ServerCapabilities, ServerInfo, Implementation,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use axum::http::request;
use tracing::info;

// Use re-exported items from http module
use crate::mcp::http::{
    HCM_BASE_URL,
    HCM_PASSWORD,
};

// Tool modules and commonly used tool types
use crate::mcp::tools::{
    absence_balance::{self, AbsenceBalanceRequest},
    projected_balance,
    absence_types,
    person_id::{self, Employee},
};

#[derive(Clone)]
pub struct OracleHCMMCPFactory {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl OracleHCMMCPFactory {
    pub fn new() -> Result<Self> {
        // Eagerly evaluate required LazyLock configurations to ensure they're valid
        // This fails fast if any required config is missing or invalid
        
        // Required: Base URL for Oracle HCM API
        let _ = HCM_BASE_URL.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_BASE_URL: {e}"))?;
        
        // Required: Password for authentication
        let _ = HCM_PASSWORD.as_ref()
            .map_err(|e| anyhow!("Failed to load HCM_PASSWORD: {e}"))?;

        // Initialize with tool router loaded from macro-generated code
        Ok(Self { tool_router: Self::tool_router() })
    }

    // Thin delegating methods so the `tool_router` proc-macro (which scans
    // this impl block) can discover and register the tools. These simply
    // forward to the actual implementations in `mcp::tools::*` so the
    // implementation remains modular.

    #[tool(
        description = "Get all available absence balances for a particular employee, based on their PersonId (the balances are based off a system calculation date, and not projected balances)."
    )]
    async fn get_all_absence_balances_for_employee_hcm_person_id(
        &self,
        params: Parameters<AbsenceBalanceRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        absence_balance::get_all_absence_balances_for_employee_hcm_person_id(params).await
    }

    #[tool(
        description = "Get projected balance for a particular PersonId as well as a projection date/effective date in DD-MM-YYYY format (Balance As Of Date), for a particular AbsenceTypeId"
    )]
    async fn get_projected_balance(
        &self,
        params: Parameters<AbsenceBalanceRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        projected_balance::get_projected_balance(params).await
    }

    #[tool(
        description = "Get the absence type IDs, and Employer IDs which are available in Oracle HCM for a particular employee, based on their PersonId. This data is used during projection of employee absence balances."
    )]
    async fn get_absence_types_for_employee_hcm_person_id(
        &self,
        params: Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        absence_types::get_absence_types_for_employee_hcm_person_id(params).await
    }

    #[tool(
        description = "Get Oracle HCM PersonId for a provided Westpac M/F/L id. Example: M061230 is a Westpac Employee ID, but it's corresponding PersonId in Oracle HCM is needed for API/or other Tool calls to HCM."
    )]
    async fn get_oracle_hcm_person_id_from_westpac_id(
        &self,
        params: Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        person_id::get_oracle_hcm_person_id_from_westpac_id(params).await
    }
}

#[tool_handler]
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
