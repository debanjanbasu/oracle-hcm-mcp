//! Tool for mapping Westpac employee IDs to Oracle HCM person IDs.
//!
//! This module provides a lookup tool that converts Westpac's internal
//! employee identifiers (e.g., M061230) to Oracle HCM's person IDs
//! which are required for other API operations.

use crate::mcp::{error::HcmError, http::{hcm_api_call, HCM_BASE_URL, HCM_API_VERSION, Method}};
use anyhow::Result;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorCode},
    ErrorData,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use serde_json::json;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Employee {
    #[schemars(description = "Unique Westpac Employee ID, e.g. M061230")]
    pub wbc_employee_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Unique PersonID in Oracle HCM, e.g. 300000578701661")]
    pub hcm_person_id: Option<String>,
}

pub async fn get_oracle_hcm_person_id_from_westpac_id(
    Parameters(args): Parameters<Employee>,
) -> Result<CallToolResult, ErrorData> {
    if args.wbc_employee_id.is_empty() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Westpac Employee ID cannot be empty.".to_string(),
            None,
        ));
    }

    let westpac_employee_id_uppercase = args.wbc_employee_id.to_uppercase();
    let base = HCM_BASE_URL
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;
    let api_ver = HCM_API_VERSION
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;

    let url = format!(
        "{base}/hcmRestApi/resources/{api_ver}/publicWorkers?q=assignments.WorkerNumber='{westpac_employee_id_uppercase}'&onlyData=true&limit=1"
    );

    let response_json = hcm_api_call(&url, Method::GET, None, true, None)
        .await?;

    let person_id = response_json["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["PersonId"].as_str());

    person_id.map_or_else(
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
        |id| Ok(CallToolResult::structured(json!({ "PersonId": id }))),
    )
}