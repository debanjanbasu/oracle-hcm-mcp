//! Tool for calculating projected future leave balances.
//!
//! This module provides functionality to calculate what an employee's
//! leave balance will be at a future date, taking into account:
//! - Current balance
//! - Accrual rates
//! - Scheduled leave
//! - Public holidays
//! - Other adjustments

use crate::mcp::{
    error::HcmError,
    http::{hcm_api_call, HCM_BASE_URL, HCM_API_VERSION},
    tools::absence_balance::AbsenceBalanceRequest,
};
use anyhow::Result;
use chrono::{NaiveDate, Local};
use crate::mcp::http::{Body, Method};
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorCode},
    ErrorData,
    tool,
};
use serde_json::json;
use std::time::Duration;

#[tool(
    description = "Get projected balance for a particular PersonId as well as a projection date/effective date in DD-MM-YYYY format (Balance As Of Date), for a particular AbsenceTypeId"
)]
pub async fn get_projected_balance(
    Parameters(AbsenceBalanceRequest {
        hcm_person_id,
        legal_entity_id,
        absence_type_id,
        balance_as_of_date,
    }): Parameters<AbsenceBalanceRequest>,
) -> Result<CallToolResult, ErrorData> {
    let base = HCM_BASE_URL
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;
    let api_ver = HCM_API_VERSION
        .as_ref()
        .map_err(|e| ErrorData::from(HcmError::MissingConfig(e.to_string())))?;

    let url = format!(
        "{base}/hcmRestApi/resources/{api_ver}/absences/action/loadProjectedBalance"
    );

    let formatted_balance_as_of_date = balance_as_of_date
        .as_ref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%d-%m-%Y").ok())
        .unwrap_or_else(|| Local::now().date_naive())
        .format("%Y-%m-%d")
        .to_string();

    let request_body = json!({
        "entry": {
            "personId": hcm_person_id,
            "legalEntityId": legal_entity_id,
            "absenceTypeId": absence_type_id,
            "openEndedFlag": "N",
            "startDate": formatted_balance_as_of_date,
            "endDate": formatted_balance_as_of_date,
            "uom": "H",
            "duration": 7.6,
            "startDateDuration": 7.6,
            "endDateDuration": 7.6
        }
    });

    let body = Body::from(serde_json::to_string(&request_body).map_err(HcmError::from)?);

    let json = hcm_api_call(
        &url,
        Method::POST,
        Some(body),
        true,
        Some(Duration::from_secs(60)),
    )
    .await?;

    let projected_balance = json["result"]["formattedProjectedBalance"]
        .as_str()
        .map_or_else(
            || {
                Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Failed to parse projected balance from response.".to_string(),
                    None,
                ))
            },
            |balance| Ok(balance.to_string()),
        )?;

    Ok(CallToolResult::structured(json!({
        "absence_type_id": absence_type_id,
        "projected_balance": projected_balance
    })))
}