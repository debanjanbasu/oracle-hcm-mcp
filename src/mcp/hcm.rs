use anyhow::{Result, anyhow};
use axum::http::request;
use reqwest::{Body, Response};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        CallToolResult, Content, GetPromptRequestParam, GetPromptResult, Implementation,
        InitializeRequestParam, InitializeResult, ListPromptsResult, PaginatedRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    prompt_handler, prompt_router,
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::LazyLock;
use tracing::info;

// The version of the HCM API to use - the latest during development is 11.13.18.05, so defaulting to it
static HCM_BASE_URL: LazyLock<String> =
    LazyLock::new(|| env::var("HCM_BASE_URL").unwrap_or_default());
static HCM_API_VERSION: LazyLock<String> =
    LazyLock::new(|| env::var("HCM_API_VERSION").unwrap_or_else(|_| "11.13.18.05".to_string()));
static REST_FRAMEWORK_VERSION: LazyLock<String> =
    LazyLock::new(|| env::var("REST_FRAMEWORK_VERSION").unwrap_or_else(|_| "9".to_string()));
// Credentials to communicate with HCM
static HCM_USERNAME: LazyLock<String> =
    LazyLock::new(|| env::var("HCM_USERNAME").unwrap_or_else(|_| "WBC_HR_AGENT".to_string()));
static HCM_PASSWORD: LazyLock<String> =
    LazyLock::new(|| env::var("HCM_PASSWORD").unwrap_or_default());

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Employee {
    // This is the unique identifier for the employee
    wbc_employee_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hcm_person_id: Option<String>,
}

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

    async fn hcm_api_call(&self, url: &str, method: &str, body: Option<Body>) -> Result<Response> {
        let client = reqwest::Client::new();
        let request_builder = match method {
            "GET" => client.get(url),
            "POST" => {
                let mut builder = client.post(url);
                if let Some(b) = body {
                    builder = builder.body(b);
                }
                builder
            }
            _ => return Err(anyhow!("Unsupported HTTP method")),
        };

        // For now we're using basic auth, but this could be extended to support OAuth in the future when enabled in HCM
        Ok(request_builder
            .basic_auth(&*HCM_USERNAME, Some(&*HCM_PASSWORD))
            .header("REST-Framework-Version", &*REST_FRAMEWORK_VERSION)
            .send()
            .await?)
    }

    #[tool(description = "Get Oracle HCM PersonId for a provided Westpac M/F/L id")]
    async fn get_oracle_hcm_person_id_from_westpac_id(
        &self,
        Parameters(args): Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        // Construct the URL to query HCM for the PersonId based on the provided Westpac Employee ID
        let url = format!(
            "{:?}/hcmRestApi/resources/{:?}/publicWorkers?q=assignments.WorkerNumber='{}'&onlyData=true&limit=1",
            HCM_BASE_URL, HCM_API_VERSION, args.wbc_employee_id
        );

        // Make the GET request to HCM with basic authentication
        let hcm_person_id = match self.hcm_api_call(&url, "GET", None).await {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => json
                    .get("items")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("PersonId"))
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string()),
                Err(_) => None,
            },
            Err(_) => None,
        };

        Ok(CallToolResult::success(vec![Content::text(
            hcm_person_id.unwrap_or_else(|| "PersonID not found".to_string()),
        )]))
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
            instructions: Some(
                "Oracle HCM (also know as People HQ) MCP Server with tools prompts and resources"
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
