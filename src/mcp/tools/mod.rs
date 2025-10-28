//! MCP tools for interacting with Oracle HCM API.
//!
//! Each module implements a specific tool:
//! - `absence_balance`: Get and calculate leave balances
//! - `absence_types`: Query available absence/leave types
//! - `person_id`: Map Westpac IDs to HCM person IDs
//! - `projected_balance`: Calculate future leave balances
//!
//! All tools use the shared HTTP client and error handling.

pub mod absence_balance;
pub mod absence_types;
pub mod person_id;
pub mod projected_balance;