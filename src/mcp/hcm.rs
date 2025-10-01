use std::{env, sync::LazyLock};
use anyhow::{anyhow, Result};
use axum::http::request;
use chrono::Local;
use reqwest::{Body, Method, Response};
use rmcp::{
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        CallToolResult, Content, ErrorCode, GetPromptRequestParam, GetPromptResult, Implementation,
        InitializeRequestParam, InitializeResult, ListPromptsResult, PaginatedRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    prompt_handler, prompt_router,
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

// Custom error enum for HCM errors
#[derive(Error, Debug)]
pub enum HcmError {
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl From<HcmError> for ErrorData {
    fn from(err: HcmError) -> Self {
        match err {
            HcmError::InvalidParams(msg) => ErrorData::new(ErrorCode::INVALID_PARAMS, msg, None),
            HcmError::Internal(e) => ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
        }
    }
}

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
    #[schemars(description = "Unique Westpac Employee ID, e.g. M061230")]
    wbc_employee_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Unique People ID in Oracle HCM, e.g. 300000578701661")]
    hcm_person_id: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AbsenceBalanceRequest {
    #[schemars(description = "Unique People ID in Oracle HCM, e.g. 300000578701661")]
    hcm_person_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Effective date for the balance in DD-MM-YYYY format, e.g. 31-12-2025. Defaults to the HCM's system calculated date if not provided.")]
    effective_date: Option<String>,
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

    async fn hcm_api_call(
        &self,
        url: &str,
        method: Method,
        body: Option<Body>,
        enable_framework_version: bool,
    ) -> Result<Response> {
        let client = reqwest::Client::new();
        // Build the request based on the HTTP method
        let mut request_builder = match method {
            Method::GET => client.get(url),
            Method::POST => client.post(url).body(body.unwrap_or_default()), // Attach body for POST requests
            _ => return Err(anyhow!("Unsupported HTTP method")), // Return error for unsupported methods
        };

        // Apply basic authentication using predefined HCM credentials
        request_builder = request_builder
            .basic_auth(&*HCM_USERNAME, Some(&*HCM_PASSWORD));

        // Conditionally add REST-Framework-Version header
        if enable_framework_version {
            request_builder = request_builder.header("REST-Framework-Version", &*REST_FRAMEWORK_VERSION);
        }

        // Finally, send the request and return the response
        Ok(request_builder.send().await?)
    }

    #[tool(description = "Get Oracle HCM PersonId for a provided Westpac M/F/L id. Example: M061230 is a Westpac Employee ID, but it's corresponding PersonId in Oracle HCM is needed for API/or other Ttool calls to HCM.")]
    async fn get_oracle_hcm_person_id_from_westpac_id(
        &self,
        Parameters(args): Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
            let westpac_employee_id_uppercase = args.wbc_employee_id.to_uppercase();
            // Construct the URL for fetching public workers, filtering by worker number
            // We'll limit to 1 result as WorkerNumber should be unique
            let url = format!(
                "{}/hcmRestApi/resources/{}/publicWorkers?q=assignments.WorkerNumber='{}'&onlyData=true&limit=1",
                &*HCM_BASE_URL, &*HCM_API_VERSION, westpac_employee_id_uppercase
            );

            // Make the API call and parse the JSON response
            let resp = self.hcm_api_call(&url, Method::GET, None, true).await.map_err(HcmError::Internal)?;
            let json: serde_json::Value = resp.json().await.map_err(|e| HcmError::Internal(e.into()))?;

            // Extract the PersonId from the JSON response using a more concise approach
            let hcm_person_id = json["items"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|item| item["PersonId"].as_str());

            // Create the result content based on whether PersonId was found
            let result_content = match hcm_person_id {
                Some(id) => Content::text(id),
                None => Content::text(format!(
                    "PersonID not found for Westpac Employee ID: {}",
                    args.wbc_employee_id
                )),
            };

            Ok(CallToolResult::success(vec![result_content]))
    }

    #[tool(description = "Get the absence types which are available in Oracle HCM for a particular employee, based on their PersonId")]
    async fn get_absence_types_for_employee_hcm_person_id(
        &self,
        Parameters(args): Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        // Ensure hcm_person_id is provided, otherwise return an InvalidParams error
        let person_id = args.hcm_person_id.ok_or_else(|| {
            HcmError::InvalidParams("HCM PersonId is required to fetch absence types.".to_string())
        })?;

        // Construct the URL for fetching absence types, filtering by PersonId
        let url = format!(
            "{}/hcmRestApi/resources/{}/absenceTypesLOV?finder=findByWord;PersonId={}",
            &*HCM_BASE_URL, &*HCM_API_VERSION, person_id
        );

        // Make the API call and parse the JSON response
        let resp = self.hcm_api_call(&url, Method::GET, None, true).await.map_err(HcmError::Internal)?;
        let json: serde_json::Value = resp.json().await.map_err(|e| HcmError::Internal(e.into()))?;

        // Extract and format absence types from the JSON response
        let result_contents: Vec<Content> = json["items"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let id = item["AbsenceTypeId"].as_str();
                        let name = item["AbsenceTypeName"].as_str();
                        match (id, name) {
                            (Some(id), Some(name)) => Some(Content::text(format!("{}: {}", id, name))),
                            _ => None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![Content::text("No absence types found".to_string())]);

        Ok(CallToolResult::success(result_contents))
    }

    #[tool(description = "Get all absence balances for a particular employee, based on their PersonId and optionally an effective date")]
    async fn get_all_absence_balances_for_employee_hcm_person_id(
        &self,
        Parameters(AbsenceBalanceRequest { hcm_person_id, mut effective_date }): Parameters<AbsenceBalanceRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Use the provided effective_date or default to today's date in DD-MM-YYYY format
        // effective_date = Some(effective_date.unwrap_or_else(|| {
        //     Local::now().format("%d-%m-%Y").to_string()
        // }));

        // Construct the URL for fetching plan balances, filtering by PersonId and active plans
        let url = format!(
            "{}/hcmRestApi/resources/{}/planBalances?q=personId={};planDisplayStatusFlag='true'",
            &*HCM_BASE_URL, &*HCM_API_VERSION, hcm_person_id
        );

        // Make the API call and parse the JSON response
        let resp = self.hcm_api_call(&url, Method::GET, None, false).await.map_err(HcmError::Internal)?;
        let json: serde_json::Value = resp.json().await.map_err(|e| HcmError::Internal(e.into()))?;
        
        // Extract and format absence balances from the JSON response
        let result_contents: Vec<Content> = json["items"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let name = item["planName"].as_str();
                        let carry_over = item["multiYearCarryOverFlag"].as_bool();
                        let plan_status = item["planStatusMeaning"].as_str();
                        let formatted_balance = item["formattedBalance"].as_str();
                        let balance_calc_date = item["balanceCalculationDate"]
                            .as_str()
                            .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                            .map(|d| d.format("%d-%m-%Y").to_string());

                        match (name, carry_over, plan_status, formatted_balance, balance_calc_date) {
                            (Some(name), Some(carry_over), Some(plan_status), Some(formatted_balance), Some(balance_calc_date)) => {
                                Some(Content::text(format!(
                                    "Plan Name: \"{}\", Carry Over: {}, Plan Status: \"{}\", Formatted Balance: \"{}\", Balance Calculation Date: \"{}\"",
                                    name, carry_over, plan_status, formatted_balance, balance_calc_date
                                )))
                            }
                            _ => None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![Content::text("No absence types found".to_string())]);

        Ok(CallToolResult::success(result_contents))
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
