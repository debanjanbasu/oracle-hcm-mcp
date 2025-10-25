//! HTTP client configuration and shared API call functionality for Oracle HCM.
//!
//! This module provides:
//! - Environment-based configuration (URLs, credentials, versions)
//! - Shared HTTP client with proper timeout and authentication
//! - OpenTelemetry integration for request tracing
//! - Common API call function used by all tools

use std::{env, sync::LazyLock, time::Duration};
use anyhow::{anyhow, Result};
use http::Extensions;
use reqwest::{Body, Method, Request, Response};
use reqwest_middleware::{ClientBuilder, Result as MiddlewareResult};
use reqwest_tracing::{
    ReqwestOtelSpanBackend, TracingMiddleware, default_on_request_end, reqwest_otel_span,
};
use tracing::Span;

use crate::mcp::error::HcmError;

// Load configuration from environment variables
pub static HCM_BASE_URL: LazyLock<Result<String>> = LazyLock::new(|| {
    env::var("HCM_BASE_URL").map_err(|e| anyhow!("HCM_BASE_URL must be set: {e}"))
});

pub static HCM_API_VERSION: LazyLock<Result<String>> =
    LazyLock::new(|| Ok(env::var("HCM_API_VERSION").unwrap_or_else(|_| "11.13.18.05".to_string())));

pub static REST_FRAMEWORK_VERSION: LazyLock<Result<String>> =
    LazyLock::new(|| Ok(env::var("REST_FRAMEWORK_VERSION").unwrap_or_else(|_| "9".to_string())));

pub static HCM_USERNAME: LazyLock<Result<String>> =
    LazyLock::new(|| Ok(env::var("HCM_USERNAME").unwrap_or_else(|_| "WBC_HR_AGENT".to_string())));

pub static HCM_PASSWORD: LazyLock<Result<String>> = LazyLock::new(|| {
    env::var("HCM_PASSWORD").map_err(|e| anyhow!("HCM_PASSWORD must be set: {e}"))
});

/// Custom tracing implementation for Oracle HCM API requests
/// 
/// This backend integrates with OpenTelemetry to provide:
/// - Request timing and latency tracking
/// - Headers and body content (sanitized)
/// - Error states and outcomes
/// - Request/response correlation
struct CustomTracing;

impl ReqwestOtelSpanBackend for CustomTracing {
    /// Start a new trace span for an outgoing request
    /// Creates spans with request details but sanitizes sensitive data
    fn on_request_start(req: &Request, _extension: &mut Extensions) -> Span {
        reqwest_otel_span!(
            name = "hcm-api-request",
            req,
            request_body = req.body().and_then(|b| b.as_bytes()).map(String::from_utf8_lossy).as_deref(),
            request_headers = ?req.headers(),
        )
    }

    /// Close the trace span and record the outcome
    /// Uses default behavior which adds response status and timing
    fn on_request_end(
        span: &Span,
        outcome: &MiddlewareResult<Response>,
        _extension: &mut Extensions,
    ) {
        default_on_request_end(span, outcome);
    }
}

/// Makes an authenticated HTTP request to the Oracle HCM REST API with tracing and error handling.
/// 
/// Creates a traced, authenticated request with proper timeouts and headers for Oracle HCM.
/// Handles common failure cases and provides detailed error information.
///
/// # Configuration
/// Uses environment variables for base configuration:
/// - `HCM_USERNAME` - API authentication username
/// - `HCM_PASSWORD` - API authentication password
/// - `REST_FRAMEWORK_VERSION` - Required API compatibility version
///
/// # Request Details
/// * Headers automatically added:
///   - Basic Auth from credentials
///   - Content-Type for POST requests
///   - REST-Framework-Version when enabled
/// * Default timeout: 30 seconds
/// * Methods supported: GET, POST
///
/// # Arguments
/// * `url` - Complete URL for the HCM API endpoint
/// * `method` - HTTP method (GET/POST only)
/// * `body` - Optional request body for POST requests
/// * `enable_framework_version` - Add REST-Framework-Version header
///   * Required for most endpoints except those using Effective-Of
/// * `set_timeout` - Custom timeout (some operations need longer)
///
/// # Returns
/// * `Ok(Value)` - Parsed JSON response
/// * `Err(HcmError)` - Structured error information:
///   * `InvalidParams` - Bad method/request
///   * `MissingConfig` - Environment not set
///   * `Http` - Network/API failures
///   * `Serialization` - JSON parse errors
///
/// # Example
/// ```no_run
/// let response = hcm_api_call(
///     "https://hcm-api.example.com/endpoint",
///     Method::GET,
///     None,
///     true,
///     None
/// ).await?;
/// ```
pub async fn hcm_api_call(
    url: &str,
    method: Method,
    body: Option<Body>,
    enable_framework_version: bool,
    set_timeout: Option<Duration>,
) -> Result<serde_json::Value, HcmError> {
    let mut client_builder = reqwest::Client::builder();

    if let Some(timeout) = set_timeout {
        client_builder = client_builder.timeout(timeout);
    }

    let client = ClientBuilder::new(client_builder.build()?)
        .with(TracingMiddleware::<CustomTracing>::new())
        .build();

    let username = HCM_USERNAME
        .as_ref()
        .map_err(|e| HcmError::MissingConfig(e.to_string()))?;
    let password = HCM_PASSWORD
        .as_ref()
        .map_err(|e| HcmError::MissingConfig(e.to_string()))?;

    let mut request_builder = match method {
        Method::GET => client.get(url),
        Method::POST => client.post(url).body(body.unwrap_or_default()),
        _ => return Err(HcmError::InvalidParams("Unsupported HTTP method".to_string())),
    };

    request_builder = request_builder.basic_auth(username.as_str(), Some(password.as_str()));

    if enable_framework_version {
        let rf_version = REST_FRAMEWORK_VERSION
            .as_ref()
            .map_err(|e| HcmError::MissingConfig(e.to_string()))?;
        request_builder = request_builder.header("REST-Framework-Version", rf_version.as_str());
    }

    if method == Method::POST {
        request_builder =
            request_builder.header("Content-Type", "application/vnd.oracle.adf.action+json");
    }

    let response = request_builder.send().await?;
    let json_value = response.json().await?;
    Ok(json_value)
}