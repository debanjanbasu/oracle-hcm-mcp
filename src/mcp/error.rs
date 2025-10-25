//! Error types for the Oracle HCM MCP implementation.
//!
//! This module contains the custom error types and conversions needed for the HCM API.
//! It provides:
//! - `HcmError`: The main error enum that covers all possible error cases
//! - Conversions from various error types (`reqwest`, `serde_json`, etc.)
//! - Conversion to RMCP's `ErrorData` for MCP protocol compliance

use reqwest_middleware;
use rmcp::ErrorData;
use thiserror::Error;
use reqwest;
use serde_json;
use rmcp::model::ErrorCode;

#[derive(Error, Debug)]
pub enum HcmError {
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Missing configuration: {0}")]
    MissingConfig(String),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP middleware error: {0}")]
    HttpMiddleware(#[from] reqwest_middleware::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl From<HcmError> for ErrorData {
    fn from(err: HcmError) -> Self {
        match err {
            HcmError::InvalidParams(msg) => Self::new(ErrorCode::INVALID_PARAMS, msg, None),
            HcmError::MissingConfig(msg) => Self::new(ErrorCode::INTERNAL_ERROR, msg, None),
            HcmError::Http(e) => Self::new(ErrorCode::INTERNAL_ERROR, format!("HTTP error: {e}"), None),
            HcmError::HttpMiddleware(e) => Self::new(ErrorCode::INTERNAL_ERROR, format!("HTTP middleware error: {e}"), None),
            HcmError::Serialization(e) => Self::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
            HcmError::Internal(e) => Self::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
        }
    }
}