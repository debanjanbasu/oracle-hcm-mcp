//! Tool for retrieving available absence types from Oracle HCM.
//!
//! This module provides a tool to list all absence/leave types
//! that are available for a given employee. This includes:
//! - Standard leave types (annual, sick, etc.)
//! - Special leave types (study, jury duty, etc.)
//! - Location-specific leave types

use crate::mcp::{
    error::HcmError,
    http::{hcm_api_call, Method},
    tools::person_id::Employee,
};
use anyhow::Result;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
    ErrorData,
};
use serde_json::json;

pub async fn get_absence_types_for_employee_hcm_person_id(
    Parameters(args): Parameters<Employee>,
) -> Result<CallToolResult, ErrorData> {
    let person_id = args
        .hcm_person_id
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            HcmError::InvalidParams("HCM PersonId is required and cannot be empty.".to_string())
        })?;

    let path = format!(
        "/absenceTypesLOV?onlyData=true&finder=findByWord;PersonId={person_id}"
    );

    let json = hcm_api_call(&path, Method::GET, None, true, None)
        .await?;

    let absence_types = json["items"]
        .as_array()
        .map(|arr| {
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
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(CallToolResult::structured(json!({ "absence_types": absence_types })))
}