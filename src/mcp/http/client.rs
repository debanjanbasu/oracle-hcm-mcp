//! HTTP client configuration and shared API call functionality for Oracle HCM.
//!
//! This module provides the core HTTP infrastructure for communicating with Oracle HCM's REST API:
//! - **Configuration Management**: Environment-based setup with sensible defaults
//! - **Connection Pooling**: Shared HTTP client for optimal performance
//! - **Request Tracing**: OpenTelemetry integration for observability
//! - **Error Handling**: Comprehensive error types and recovery
//! - **Authentication**: Basic auth with credential management
//!
//! # Configuration
//! All configuration is loaded from environment variables at startup:
//! - `HCM_BASE_URL` (required): Base URL for your Oracle HCM instance
//! - `HCM_API_VERSION` (optional): API version, defaults to "11.13.18.05"
//! - `HCM_USERNAME` (optional): Username, defaults to "`WBC_HR_AGENT`"
//! - `HCM_PASSWORD` (required): Password for authentication
//! - `REST_FRAMEWORK_VERSION` (optional): Framework version, defaults to "9"
//!
//! # Performance
//! The module uses a singleton HTTP client (via `LazyLock`) that is initialized once
//! and reused across all requests. This provides:
//! - Connection pooling and reuse
//! - Reduced memory allocations
//! - Lower latency for subsequent requests

use std::{env, sync::LazyLock, time::Duration};
use anyhow::{anyhow, Result};
use http::Extensions;
use reqwest::{Body, Method, Request, Response};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Result as MiddlewareResult};
use reqwest_tracing::{
    ReqwestOtelSpanBackend, TracingMiddleware, default_on_request_end, reqwest_otel_span,
};
use tracing::{Span, error, info, trace};

use crate::mcp::error::HcmError;

/// Helper function to load and sanitize environment variables.
/// Removes surrounding quotes (both single and double) that may be added by shell or .env files.
fn load_env_var(key: &str) -> Result<String> {
    env::var(key)
        .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string())
        .map_err(|e| anyhow!("{key} must be set: {e}"))
}

/// Helper function to load optional environment variables with a default value.
/// Removes surrounding quotes that may be added by shell or .env files.
fn load_env_var_or(key: &str, default: &str) -> String {
    env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .trim_matches('"')
        .to_string()
}

// ============================================================================
// Configuration - Loaded once at startup
// ============================================================================

/// Base URL for the Oracle HCM instance (e.g., "<https://your-instance.oraclecloud.com>")
/// This is required and must be set via environment variable.
pub static HCM_BASE_URL: LazyLock<Result<String>> = 
    LazyLock::new(|| load_env_var("HCM_BASE_URL"));

/// Oracle HCM API version used in request paths.
/// Defaults to "11.13.18.05" if not specified.
pub static HCM_API_VERSION: LazyLock<String> = 
    LazyLock::new(|| load_env_var_or("HCM_API_VERSION", "11.13.18.05"));

/// REST Framework Version header value.
/// Required by most HCM API endpoints. Defaults to "9" if not specified.
pub static REST_FRAMEWORK_VERSION: LazyLock<String> = 
    LazyLock::new(|| load_env_var_or("REST_FRAMEWORK_VERSION", "9"));

/// Username for Basic Authentication with Oracle HCM.
/// Defaults to "`WBC_HR_AGENT`" if not specified.
pub static HCM_USERNAME: LazyLock<String> = 
    LazyLock::new(|| load_env_var_or("HCM_USERNAME", "WBC_HR_AGENT"));

/// Password for Basic Authentication with Oracle HCM.
/// This is required and must be set via environment variable.
pub static HCM_PASSWORD: LazyLock<Result<String>> = 
    LazyLock::new(|| load_env_var("HCM_PASSWORD"));

// ============================================================================
// HTTP Client - Singleton with connection pooling
// ============================================================================

/// Shared HTTP client with middleware for tracing.
/// Initialized lazily on first use and reused for all subsequent requests.
/// Uses a 30-second timeout by default.
///
/// Returns a `Result` to handle initialization failures gracefully
/// (e.g., TLS configuration issues).
static HTTP_CLIENT: LazyLock<Result<ClientWithMiddleware, String>> = LazyLock::new(|| {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    
    Ok(ClientBuilder::new(client)
        .with(TracingMiddleware::<CustomTracing>::new())
        .build())
});

// ============================================================================
// Request Tracing - OpenTelemetry integration
// ============================================================================

/// Custom tracing backend for Oracle HCM API requests.
/// 
/// Integrates with OpenTelemetry to provide comprehensive observability:
/// - **Request timing**: Measures latency for each API call
/// - **Request details**: Logs method, URL, headers, and body
/// - **Response correlation**: Links requests with their responses
/// - **Error tracking**: Captures failure states and status codes
///
/// This enables monitoring, debugging, and performance analysis in production.
struct CustomTracing;

impl ReqwestOtelSpanBackend for CustomTracing {
    /// Creates a new tracing span when an HTTP request starts.
    /// 
    /// The span captures:
    /// - Request method and URL
    /// - Request headers (for debugging auth/content-type issues)
    /// - Request body (if available)
    ///
    /// Note: Sensitive data like passwords should be filtered by the tracing layer.
    fn on_request_start(req: &Request, _extension: &mut Extensions) -> Span {
        reqwest_otel_span!(
            name = "hcm-api-request",
            req,
            request_body = req.body().and_then(|b| b.as_bytes()).map(String::from_utf8_lossy).as_deref(),
            request_headers = ?req.headers(),
        )
    }

    /// Completes the tracing span when the HTTP request finishes.
    /// 
    /// Records:
    /// - Response status code
    /// - Request duration
    /// - Success or failure outcome
    ///
    /// Uses the default implementation which handles standard HTTP metrics.
    fn on_request_end(
        span: &Span,
        outcome: &MiddlewareResult<Response>,
        _extension: &mut Extensions,
    ) {
        default_on_request_end(span, outcome);
    }
}

// ============================================================================
// HTTP API Call - Main entry point for all HCM API requests
// ============================================================================

/// Makes an authenticated HTTP request to the Oracle HCM REST API.
/// 
/// This is the primary function used by all tools to communicate with Oracle HCM.
/// It handles authentication, headers, timeouts, tracing, and error handling automatically.
///
/// # Arguments
/// * `path` - API endpoint path relative to the base URL
///   - Example: `"/publicWorkers?q=assignments.WorkerNumber='M061230'"`
///   - Should include query parameters if needed
/// * `method` - HTTP method to use (only `GET` and `POST` are supported)
/// * `body` - Request body for POST requests (use `None` for GET)
/// * `enable_framework_version` - Whether to add the `REST-Framework-Version` header
///   - Set to `true` for most endpoints
///   - Set to `false` for endpoints that use `Effective-Of` header instead
/// * `set_timeout` - Custom timeout override (use `None` for default 30s timeout)
///   - Some operations (like projected balance calculations) may need longer timeouts
///
/// # Returns
/// * `Ok(Value)` - Parsed JSON response from the API
/// * `Err(HcmError)` - Detailed error information:
///   - `InvalidParams`: Bad request (e.g., unsupported HTTP method)
///   - `MissingConfig`: Environment variable not set
///   - `Http`: Network error or API returned error status
///   - `Serialization`: Failed to parse JSON response
///
/// # Example
/// ```no_run
/// use reqwest::Method;
/// use std::time::Duration;
/// 
/// // Simple GET request with default timeout
/// let workers = hcm_api_call(
///     "/publicWorkers?onlyData=true&limit=10",
///     Method::GET,
///     None,
///     true,  // Include REST-Framework-Version header
///     None   // Use default 30s timeout
/// ).await?;
///
/// // POST request with custom timeout
/// let body = Body::from(r#"{"entry": {"personId": "123"}}"#);
/// let result = hcm_api_call(
///     "/absences/action/loadProjectedBalance",
///     Method::POST,
///     Some(body),
///     true,
///     Some(Duration::from_secs(60))  // 60s timeout for slow operation
/// ).await?;
/// ```
///
/// # URL Construction
/// The full URL is built as:
/// ```text
/// {HCM_BASE_URL}/hcmRestApi/resources/{HCM_API_VERSION}{path}
/// ```
/// For example, with:
/// - `HCM_BASE_URL=https://instance.oraclecloud.com`
/// - `HCM_API_VERSION=11.13.18.05`
/// - `path=/publicWorkers?onlyData=true`
/// 
/// The final URL will be:
/// ```text
/// https://instance.oraclecloud.com/hcmRestApi/resources/11.13.18.05/publicWorkers?onlyData=true
/// ```
///
/// # Authentication
/// Automatically adds HTTP Basic Authentication using:
/// - Username from `HCM_USERNAME` environment variable
/// - Password from `HCM_PASSWORD` environment variable
///
/// # Logging
/// Logs at different levels:
/// - `INFO`: Request/response summary with URL and status
/// - `TRACE`: Full JSON response body for debugging
/// - `ERROR`: Error details when requests fail
pub async fn hcm_api_call(
    path: &str,
    method: Method,
    body: Option<Body>,
    enable_framework_version: bool,
    set_timeout: Option<Duration>,
) -> Result<serde_json::Value, HcmError> {
    // Load configuration from static globals (loaded once at startup)
    let base = HCM_BASE_URL
        .as_ref()
        .map_err(|e| HcmError::MissingConfig(e.to_string()))?;
    let api_ver = HCM_API_VERSION.as_str();

    // Construct the full API URL
    let url = format!("{base}/hcmRestApi/resources/{api_ver}{path}");
    
    info!("HCM API request: {} {}", method, url);
    
    // Use shared client for optimal performance (connection pooling)
    let client = HTTP_CLIENT
        .as_ref()
        .map_err(|e| HcmError::Internal(anyhow!("HTTP client initialization failed: {e}")))?
        .clone();

    // Load authentication credentials
    let username = HCM_USERNAME.as_str();
    let password = HCM_PASSWORD
        .as_ref()
        .map_err(|e| HcmError::MissingConfig(e.to_string()))?;

    // Build the HTTP request based on method
    let mut request_builder = match method {
        Method::GET => client.get(&url),
        Method::POST => client.post(&url).body(body.unwrap_or_default()),
        _ => return Err(HcmError::InvalidParams("Only GET and POST methods are supported".to_string())),
    };
    
    // Apply custom timeout if specified
    if let Some(timeout) = set_timeout {
        request_builder = request_builder.timeout(timeout);
    }

    // Add HTTP Basic Authentication
    request_builder = request_builder.basic_auth(username, Some(password));

    // Add REST-Framework-Version header if requested (required by most endpoints)
    if enable_framework_version {
        let rf_version = REST_FRAMEWORK_VERSION.as_str();
        request_builder = request_builder.header("REST-Framework-Version", rf_version);
    }

    // Add Content-Type header for POST requests (Oracle ADF format)
    if method == Method::POST {
        request_builder =
            request_builder.header("Content-Type", "application/vnd.oracle.adf.action+json");
    }

    // Execute the request
    let response = request_builder.send().await?;
    let status = response.status();
    
    info!("HCM API response: {} {} - Status: {}", method, url, status);
    
    // Handle error responses
    if !status.is_success() {
        let error_text = response.text().await
            .unwrap_or_else(|e| format!("Unable to read error response body: {e}"));
        
        error!("HCM API request failed with status {}: {}", status, error_text);
        return Err(HcmError::Internal(anyhow!("HTTP {status}: {error_text}")));
    }
    
    // Parse successful JSON response
    let json_response = response.json::<serde_json::Value>().await.map_err(|e| {
        error!("HCM API failed to parse successful response as JSON: {}", e);
        HcmError::Internal(anyhow!("JSON parsing failed: {e}"))
    })?;
    
    trace!("HCM API response (JSON): {:?}", json_response);
    Ok(json_response)
}