//! Tool for retrieving available absence types from Oracle HCM.
//!
//! This module provides a tool to list all absence/leave types
//! that are available for a given employee. This includes:
//! - Standard leave types (annual, sick, etc.)
//! - Special leave types (study, jury duty, etc.)
//! - Location-specific leave types

use crate::mcp::{
    error::HcmError,
    http::{hcm_api_call, HCM_BASE_URL, HCM_API_VERSION},
    tools::person_id::Employee,
};
use anyhow::Result;
use reqwest::Method;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    ErrorData,
    tool,
};
use serde_json::json;

#[tool(
    description = "Get the absence type IDs, and Employer IDs which are available in Oracle HCM for a particular employee, based on their PersonId. This data is used during projection of employee absence balances."
)]
pub async fn get_absence_types_for_employee_hcm_person_id(
    Parameters(args): Parameters<Employee>,
) -> Result<CallToolResult, ErrorData> {
    let person_id = args
        .hcm_person_id
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            HcmError::InvalidParams("HCM PersonId is required and cannot be empty.".to_string())
        })?;

    let base = HCM_BASE_URL
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;
    let api_ver = HCM_API_VERSION
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;

    let url = format!(
        "{base}/hcmRestApi/resources/{api_ver}/absenceTypesLOV?onlyData=true&finder=findByWord;PersonId={person_id}"
    );

    let json = hcm_api_call(&url, Method::GET, None, true, None)
        .await?;

    let absence_types: Vec<serde_json::Value> =
        json["items"].as_array().map_or_else(Vec::new, |arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item["AbsenceTypeId"].as_str()?;
                    let employer_id = item["EmployerId"].as_str()?;
                    let name = item["AbsenceTypeName"].as_str()?;
                    Some(json!({
                        "AbsenceTypeId": id,
                        "EmployerId": employer_id,
                        "AbsenceTypeName": name
                    }))
                })
                .collect()
        });

    Ok(CallToolResult::structured(json!({ "absence_types": absence_types })))
}