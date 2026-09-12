// ============================================================
// The settings surface of the MCP interface (TICKET-041).
//
// Everything the frontend may ask about the interface, and everything it
// may change. Deliberately small: the configuration is one object, the
// token is read and regenerated but never *set* from outside, and the
// audit log is read-only apart from clearing it.
//
// The token is the reason `mcp_status` and `mcp_token` are separate
// calls. Status is polled while the settings page is open; the token is
// a credential and is fetched only when the user asks to see it, so it
// does not sit in the frontend's memory for the life of the dialog.
// ============================================================

use super::{audit, auth, McpConfig};
use crate::errors::AppResult;
use serde::Serialize;
use ts_rs::TS;

/// What the settings page shows about the interface.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpStatus {
    /// The stored configuration.
    pub config: McpConfig,
    /// Whether the listener is actually up right now.
    pub running: bool,
    /// The port it is listening on, once it is.
    #[ts(type = "number | null")]
    pub bound_port: Option<u16>,
    /// The address a client is configured with, once it is listening.
    pub endpoint: Option<String>,
    /// Whether a token exists yet — without revealing it.
    pub has_token: bool,
}

/// The current configuration and listener state.
#[tauri::command]
pub fn mcp_status() -> McpStatus {
    let config = super::config();
    let has_token = matches!(
        crate::net::credentials::CredentialStore::get(
            crate::net::credentials::store(),
            "__mcp_token__"
        ),
        Ok(Some(ref t)) if !t.is_empty()
    );
    McpStatus {
        running: false,
        bound_port: None,
        endpoint: None,
        has_token,
        config,
    }
}

/// Replaces the configuration; returns what was actually stored.
///
/// The returned value is the validated one, not the one that was sent —
/// permanent deletion is cleared without its tier, the read cap is
/// clamped, and the roots are tidied — so the settings page shows what
/// is in force rather than what was asked for.
#[tauri::command]
pub fn mcp_set_config(config: McpConfig) -> AppResult<McpConfig> {
    super::set_config(config)
}

/// The bearer token, generating one on first use.
#[tauri::command]
pub fn mcp_token() -> AppResult<String> {
    auth::token()
}

/// A new token. The old one stops working immediately.
#[tauri::command]
pub fn mcp_regenerate_token() -> AppResult<String> {
    auth::regenerate()
}

/// The audit log, newest first.
#[tauri::command]
pub fn mcp_audit() -> Vec<audit::McpAuditEntry> {
    audit::entries()
}

/// Empties the audit log.
#[tauri::command]
pub fn mcp_clear_audit() {
    audit::clear();
}
