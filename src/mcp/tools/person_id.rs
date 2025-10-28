//! Tool for mapping Westpac employee IDs to Oracle HCM person IDs.
//!
//! Oracle HCM uses numeric person IDs (e.g., "300000578701661") as the primary identifier
//! for employees. However, Westpac uses alphanumeric employee IDs (e.g., "M061230").
//! This tool provides the mapping between these two identifier systems.
//!
//! # Use Case
//! Most HCM API operations require a person ID. When an AI assistant knows only
//! the Westpac employee ID, this tool must be called first to obtain the corresponding
//! HCM person ID before calling other tools.
//!
//! # API Details
//! Uses the `/publicWorkers` endpoint with a filter on `assignments.WorkerNumber`.

use crate::mcp::http::{hcm_api_call, Method};
use anyhow::Result;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorCode},
    ErrorData,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use serde_json::json;

/// Input parameter for person ID lookup.
/// Contains the Westpac employee ID to search for.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct Employee {
    /// Westpac employee identifier (e.g., "M061230", "F123456", "L789012")
    /// Case-insensitive - will be converted to uppercase for API query
    #[schemars(description = "Unique Westpac Employee ID, e.g. M061230")]
    pub wbc_employee_id: String,
    
    /// Oracle HCM person identifier (numeric string)
    /// This field is optional in the input but required in other tools
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Unique PersonID in Oracle HCM, e.g. 300000578701661")]
    pub hcm_person_id: Option<String>,
}

/// Looks up an Oracle HCM person ID from a Westpac employee ID.
///
/// # Arguments
/// * `args` - Contains the `wbc_employee_id` to search for
///
/// # Returns
/// * `Ok(CallToolResult)` - JSON containing the `PersonId` field
/// * `Err(ErrorData)` - If employee not found or API error
///
/// # Example Response
/// ```json
/// {
///   "PersonId": "300000578701661"
/// }
/// ```
///
/// # Errors
/// * `INVALID_PARAMS` - If `wbc_employee_id` is empty or employee not found
/// * `INTERNAL_ERROR` - If API call fails
pub async fn get_oracle_hcm_person_id_from_westpac_id(
    Parameters(args): Parameters<Employee>,
) -> Result<CallToolResult, ErrorData> {
    // Validate input
    if args.wbc_employee_id.is_empty() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Westpac Employee ID cannot be empty.".to_string(),
            None,
        ));
    }

    // Build API query - convert to uppercase as HCM stores IDs in uppercase
    // Limit to 1 result since employee IDs are unique
    let path = format!(
        "/publicWorkers?q=assignments.WorkerNumber='{}'&onlyData=true&limit=1",
        args.wbc_employee_id.to_uppercase()
    );

    // Make API call with REST-Framework-Version header
    let response_json = hcm_api_call(&path, Method::GET, None, true, None)
        .await?;

    // Extract PersonId from response
    // Response format: {"items": [{"PersonId": "123", ...}]}
    let person_id = response_json["items"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["PersonId"].as_str());

    // Return PersonId or error if not found
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