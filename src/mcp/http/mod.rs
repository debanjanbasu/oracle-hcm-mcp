//! HTTP client and API communication layer for Oracle HCM.
//!
//! This module handles all HTTP communication with Oracle HCM's REST API,
//! including authentication, request construction, and response handling.

mod client;

// Re-export client's public API
pub use client::{
    hcm_api_call,
    HCM_BASE_URL,
    HCM_PASSWORD,
};

// Re-export common types used in our public API
pub use reqwest::{Body, Method};