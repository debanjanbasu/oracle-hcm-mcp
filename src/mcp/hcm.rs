use anyhow::{Result, anyhow};
use axum::http::request;
use chrono::NaiveDate;
use reqwest::{Body, Method};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        CallToolResult, ErrorCode, GetPromptRequestParam, GetPromptResult, Implementation,
        InitializeRequestParam, InitializeResult, ListPromptsResult, PaginatedRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, sync::LazyLock};
use thiserror::Error;
use tracing::{error, info};

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
            HcmError::InvalidParams(msg) => Self::new(ErrorCode::INVALID_PARAMS, msg, None),
            HcmError::Internal(e) => Self::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
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
    #[schemars(description = "Unique PersonID in Oracle HCM, e.g. 300000578701661")]
    hcm_person_id: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AbsenceBalanceRequest {
    #[schemars(description = "Unique PersonID in Oracle HCM, e.g. 300000578701661")]
    hcm_person_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Effective date (Balance As Of Date) for the balance in DD-MM-YYYY format, e.g. 31-12-2025. Defaults to the HCM's system calculated date if not provided."
    )]
    balance_as_of_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "The PlanID for the absence balance request, e.g. 300100033342761.")]
    plan_id: Option<String>,
}

#[derive(Clone)]
pub struct OracleHCMMCPFactory {
    tool_router: ToolRouter<OracleHCMMCPFactory>,
    prompt_router: PromptRouter<OracleHCMMCPFactory>,
}

#[tool_router]
impl OracleHCMMCPFactory {
    pub fn new() -> Self {
        Self {
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
    ) -> Result<serde_json::Value> {
        let client = reqwest::Client::new();
        // Build the request based on the HTTP method
        let mut request_builder = match method {
            Method::GET => client.get(url),
            Method::POST => client.post(url).body(body.unwrap_or_default()), // Attach body for POST requests
            _ => return Err(anyhow!("Unsupported HTTP method")), // Return error for unsupported methods
        };

        // Apply basic authentication using predefined HCM credentials
        request_builder = request_builder.basic_auth(&*HCM_USERNAME, Some(&*HCM_PASSWORD));

        // Conditionally add REST-Framework-Version header
        if enable_framework_version {
            request_builder =
                request_builder.header("REST-Framework-Version", &*REST_FRAMEWORK_VERSION);
        }

        // Finally, send the request and return the JSON response
        let response = request_builder.send().await?;
        Ok(response.json().await?)
    }

    #[tool(
        description = "Get Oracle HCM PersonId for a provided Westpac M/F/L id. Example: M061230 is a Westpac Employee ID, but it's corresponding PersonId in Oracle HCM is needed for API/or other Tool calls to HCM."
    )]
    async fn get_oracle_hcm_person_id_from_westpac_id(
        &self,
        Parameters(args): Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        // Use a guard clause for early return on invalid input.
        if args.wbc_employee_id.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Westpac Employee ID cannot be empty.".to_string(),
                None,
            ));
        }

        // The HCM API endpoint only accepts uppercase Westpac Employee IDs (that's how it's populated as a result of the Westpac Employee ID being converted to uppercase).
        let westpac_employee_id_uppercase = args.wbc_employee_id.to_uppercase();
        // The URL for the HCM API endpoint.
        let url = format!(
            "{}/hcmRestApi/resources/{}/publicWorkers?q=assignments.WorkerNumber='{}'&onlyData=true&limit=1",
            &*HCM_BASE_URL, &*HCM_API_VERSION, westpac_employee_id_uppercase
        );

        // Execute API call. The `?` operator will automatically propagate any `anyhow::Error`
        // which will then be converted to `HcmError::Internal` by the `From` trait
        // and subsequently to `ErrorData` by the `From<HcmError> for ErrorData` implementation.
        let response_json = self
            .hcm_api_call(&url, Method::GET, None, true)
            .await
            .map_err(HcmError::Internal)?;

        // Attempt to extract PersonId directly.
        let person_id = response_json["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["PersonId"].as_str());

        // Return the result or an error.
        person_id.map_or_else(
            // If person_id is None, return an InvalidParams error.
            || {
                Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "PersonID not found for Westpac Employee ID: {}",
                        args.wbc_employee_id
                    ),
                    None,
                ))
            },
            // If person_id is Some(id), return a success CallToolResult.
            |id| Ok(CallToolResult::structured(json!({ "PersonId": id }))),
        )
    }

    #[tool(
        description = "Get the absence types which are available in Oracle HCM for a particular employee, based on their PersonId"
    )]
    async fn get_absence_types_for_employee_hcm_person_id(
        &self,
        Parameters(args): Parameters<Employee>,
    ) -> Result<CallToolResult, ErrorData> {
        // Ensure hcm_person_id is provided and not empty, otherwise return an InvalidParams error.
        let person_id = args
            .hcm_person_id
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                HcmError::InvalidParams("HCM PersonId is required and cannot be empty.".to_string())
            })?;

        // Construct the URL for fetching absence types, filtering by PersonId
        let url = format!(
            "{}/hcmRestApi/resources/{}/absenceTypesLOV?onlyData=true&finder=findByWord;PersonId={}",
            &*HCM_BASE_URL, &*HCM_API_VERSION, person_id
        );

        // Make the API call and get the JSON response
        let json = self
            .hcm_api_call(&url, Method::GET, None, true)
            .await
            .map_err(HcmError::Internal)?;

        // Extract and format absence types from the JSON response
        let absence_types: Vec<serde_json::Value> =
            json["items"].as_array().map_or_else(Vec::new, |arr| {
                arr.iter()
                    .filter_map(|item| {
                        let id = item["AbsenceTypeId"].as_str()?;
                        let name = item["AbsenceTypeName"].as_str()?;
                        Some(json!({
                            "AbsenceTypeId": id,
                            "AbsenceTypeName": name
                        }))
                    })
                    .collect()
            });

        Ok(CallToolResult::structured(json!({
            "absence_types": absence_types
        })))
    }

    #[tool(
        description = "Get all available absence balances and their IDs for a particular employee, based on their PersonId (the balances are based off a system calculation date, and not projected balances)."
    )]
    async fn get_all_absence_balances_for_employee_hcm_person_id(
        &self,
        Parameters(args): Parameters<AbsenceBalanceRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        // Ensure hcm_person_id is not empty.
        if args.hcm_person_id.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "HCM PersonId cannot be empty.".to_string(),
                None,
            ));
        }

        // Construct the URL for fetching plan balances, filtering by PersonId
        let url = format!(
            "{}/hcmRestApi/resources/{}/planBalances?onlyData=true&q=personId={};planDisplayStatusFlag=true",
            &*HCM_BASE_URL, &*HCM_API_VERSION, args.hcm_person_id
        );

        // Make the API call and get the JSON response
        let json = self
            // Be careful here, if we pass the REST-Framework-Version: string here, it doesn't respond, as it requires a valid Effective-Of: string as a header as well
            .hcm_api_call(&url, Method::GET, None, false)
            .await
            .map_err(HcmError::Internal)?;

        // Extract and format absence balances from the JSON response
        let absence_balances: Vec<serde_json::Value> =
            json["items"].as_array().map_or_else(Vec::new, |arr| {
                arr.iter()
                    .filter_map(|item| {
                        let plan_id = item["planId"].as_u64()?;
                        let name = item["planName"].as_str()?;
                        let carry_over = item["multiYearCarryOverFlag"].as_bool()?;
                        let plan_status = item["planStatusMeaning"].as_str()?;
                        let formatted_balance = item["formattedBalance"].as_str()?;
                        let balance_calc_date = item["balanceCalculationDate"]
                            .as_str()
                            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                            // Convert from US to AUS Time Format
                            .map(|d| d.format("%d-%m-%Y").to_string())?;

                        Some(json!({
                            "planId": plan_id,
                            "planName": name,
                            "carryOver": carry_over,
                            "planStatus": plan_status,
                            "formattedBalance": formatted_balance,
                            "balanceCalculationDate": balance_calc_date,
                        }))
                    })
                    .collect()
            });

        Ok(CallToolResult::structured(json!({
          "absence_balances": absence_balances,
        })))
    }

    // #[tool(
    //     description = "Get projected balance for a particular PersonId as well as a projection date/effective date in DD-MM-YYYY format (Balance As Of Date), for a particular PlanId"
    // )]
    // async fn get_projected_balance(
    //     &self,
    //     Parameters(AbsenceBalanceRequest {
    //         hcm_person_id,
    //         plan_id,
    //         balance_as_of_date,
    //     }): Parameters<AbsenceBalanceRequest>,
    // ) -> Result<CallToolResult, ErrorData> {
    //     // Construct the URL for fetching plan balances, filtering by PersonId, PlanId, and balanceAsOfDate.
    //     let url = format!(
    //         "{}/hcmRestApi/resources/{}/absences/action/loadProjectedBalance",
    //         &*HCM_BASE_URL, &*HCM_API_VERSION,
    //     );

    //     // Build the request body
    //     let body = json!({
    //         "personId": hcm_person_id,
    //         "planId": plan_id,
    //         "balanceAsOfDate": balance_as_of_date.as_ref().map_or_else(
    //             || {
    //                 // Default to current date in yyyy-mm-dd format if not provided
    //                 let now = chrono::Local::now();
    //                 now.format("%Y-%m-%d").to_string()
    //             },
    //             |date_str| {
    //                 chrono::NaiveDate::parse_from_str(date_str, "%d-%m-%Y")
    //                     .map_or_else(|_| date_str.clone(), |d| d.format("%Y-%m-%d").to_string()) // Fallback to original if parsing fails
    //             },
    //         )
    //     });

    //     // Make the API call and get the JSON response
    //     let json = self
    //         .hcm_api_call(
    //             &url,
    //             Method::POST,
    //             Some(Body::from(serde_json::to_string(&body)?)),
    //             true,
    //         )
    //         .await
    //         .map_err(HcmError::Internal)?;

    //     // Extract and format projected balance from the JSON response
    //     let result_contents: Vec<Content> = json["items"].as_array().map_or_else(
    //             || vec![Content::text("No projected balance found for the given criteria.".to_string())],
    //             |arr| {
    //                 arr.iter()
    //                     .filter_map(|item| {
    //                         let plan_id = item["planId"].as_str()?;
    //                         let name = item["planName"].as_str()?;
    //                         let carry_over = item["multiYearCarryOverFlag"].as_bool()?;
    //                         let plan_status = item["planStatusMeaning"].as_str()?;
    //                         let formatted_balance = item["formattedBalance"].as_str()?;
    //                         let balance_calc_date = item["balanceCalculationDate"]
    //                             .as_str()
    //                             .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    //                             // Convert from US to AUS Time Format
    //                             .map(|d| d.format("%d-%m-%Y").to_string())?;

    //                         Some(Content::text(format!(
    //                             "Plan ID: {plan_id}, Plan Name: {name}, Carry Over: {carry_over}, Plan Status: {plan_status}, Formatted Balance: {formatted_balance}, Balance Calculation Date: {balance_calc_date}"
    //                         )))
    //                     })
    //                     .collect()
    //             },
    //         );

    //     Ok(CallToolResult::success(result_contents))
    // }
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
                "Oracle HCM (also know as People HQ at Westpac) MCP Server with tools prompts and resources"
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
