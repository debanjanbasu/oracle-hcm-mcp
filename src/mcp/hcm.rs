use axum::http::request;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::router::{prompt::PromptRouter, tool::ToolRouter},
    model::{
        CallToolResult, Content, GetPromptRequestParam, GetPromptResult, Implementation,
        InitializeRequestParam, InitializeResult, ListPromptsResult, PaginatedRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tracing::info;

// TODO: Implement a better version control mechanism
// possibly from env variables, and default to an older one
const ORACLE_HCM_API_VERSION: &str = "11.13.18.05";

#[derive(Clone)]
pub struct OracleHCMMCPFactory {
    tool_router: ToolRouter<OracleHCMMCPFactory>,
    prompt_router: PromptRouter<OracleHCMMCPFactory>,
}

#[tool_router]
impl OracleHCMMCPFactory {
    pub fn new() -> Self {
        OracleHCMMCPFactory {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    #[tool(description = "Get all Aternity Remediations")]
    async fn get_my_leave_balances(&self) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![
            Content::text("Remediation1".to_string()),
            Content::text("Remediation2".to_string()),
        ]))
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
                .enable_prompts()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Aternity MCP Server with tools and prompts".to_string()),
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
