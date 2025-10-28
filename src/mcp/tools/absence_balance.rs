//! Tool for retrieving current absence/leave balances from Oracle HCM.
//!
//! This module provides functionality to fetch an employee's current
//! leave balances including:
//! - Annual leave
//! - Personal/sick leave
//! - Long service leave
//! - Other leave types
//!
//! Balances are returned in hours and include carry-over status.

use crate::mcp::http::{hcm_api_call, Method};
use anyhow::Result;
use chrono::NaiveDate;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorCode},
    ErrorData,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use serde_json::json;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AbsenceBalanceRequest {
    #[schemars(description = "Unique PersonID in Oracle HCM, e.g. 300000578701661")]
    pub hcm_person_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Effective date (Balance As Of Date) for the balance in DD-MM-YYYY format, e.g. 31-12-2025. Defaults to the HCM's system calculated date if not provided."
    )]
    pub balance_as_of_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "The Absence Type ID for the absence balance request, e.g. 300001058681790."
    )]
    pub absence_type_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "The Legal Entity ID for the absence balance request, e.g. 300000001487001."
    )]
    pub legal_entity_id: Option<String>,
}

pub async fn get_all_absence_balances_for_employee_hcm_person_id(
    Parameters(args): Parameters<AbsenceBalanceRequest>,
) -> Result<CallToolResult, ErrorData> {
    if args.hcm_person_id.is_empty() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "HCM PersonId cannot be empty.".to_string(),
            None,
        ));
    }

    let path = format!(
        "/planBalances?onlyData=true&q=personId={};planDisplayStatusFlag=true",
        args.hcm_person_id
    );

    let json = hcm_api_call(&path, Method::GET, None, false, None)
        .await?;

    let absence_balances = json["items"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let name = item["planName"].as_str()?;
                    let carry_over = item["multiYearCarryOverFlag"].as_bool()?;
                    let plan_status = item["planStatusMeaning"].as_str()?;
                    let formatted_balance = item["formattedBalance"].as_str()?;
                    let balance_calc_date = item["balanceCalculationDate"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                        .map(|d| d.format("%d-%m-%Y").to_string())?;

                    Some(json!({
                        "planName": name,
                        "carryOver": carry_over,
                        "planStatus": plan_status,
                        "formattedBalance": formatted_balance,
                        "balanceCalculationDate": balance_calc_date,
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(CallToolResult::structured(json!({
        "absence_balances": absence_balances,
    })))
}