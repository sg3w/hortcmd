// ============================================================
// hortcmd as an MCP server (TICKET-041).
//
// Every MCP host ships some flavour of a local-filesystem server. What
// this one adds is the part those cannot do: since tickets 020–026 the
// application owns a VFS with live, authenticated connections,
// credentials in the OS keychain and a session registry. A client
// talking to a *running* hortcmd therefore reaches `sftp://…` or
// `smb://…` through the connection that is already open — no second
// login, and no credentials handed to the AI host.
//
//   config    – what is enabled, which roots, which tiers (this file)
//   auth      – the bearer token and the Origin/Host check
//   scope     – does this address lie inside an allowed root?
//   protocol  – JSON-RPC 2.0 framing and the MCP method names
//   server    – the localhost HTTP listener and the request loop
//   tools     – one module per tier, calling the same VFS the app does
//   audit     – a ring buffer of what was allowed, denied and confirmed
//
// **Why no SDK.** The official `rmcp` crate declares `rust-version =
// 1.88`; this project's MSRV is 1.85 and has a CI job that holds it
// (the pure-Rust SMB client needs 1.85, and raising the floor for a
// convenience is the wrong trade). Versions of `rmcp` below 3.0 declare
// no MSRV at all, which in a repository that *tests* its MSRV is worse
// than a documented mismatch. MCP's streamable-HTTP transport is
// JSON-RPC 2.0 over POST with an optional SSE stream, so the framing is
// small and the parts that matter here — the token check, the Origin
// check, tier gating, confirmation — are ours either way.
//
// **Default-deny throughout.** Off after a fresh install. No root means
// nothing is reachable. Only the read tier is on when it is switched on.
// Write and delete each need their own switch *and*, by default, a
// confirmation in the application. None of that is a substitute for the
// bind address, and the bind address is not a substitute for any of it:
// every local process can reach 127.0.0.1.
// ============================================================

pub mod audit;
pub mod auth;
pub mod commands;
pub mod scope;

use crate::config;
use crate::errors::{code, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

/// What a client is allowed to ask for.
///
/// Ordered by how much of the user's data a mistake costs, and switched
/// on separately: a client that may read is not thereby a client that
/// may overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum McpTier {
    /// Listing, stat, reading, searching, comparing — nothing changes.
    Read,
    /// Creating, writing, renaming, copying, moving, extracting.
    Write,
    /// Moving to the trash, and — only with `allow_permanent_delete` —
    /// deleting outright.
    Delete,
}

/// The persisted configuration of the interface.
///
/// Written to `<config>/hortcmd/mcp.json`; the token is *not* in here,
/// it lives in the credential store.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpConfig {
    /// Whether the server runs. Off after a fresh install.
    pub enabled: bool,
    /// The port on 127.0.0.1. 0 asks the operating system for one.
    #[ts(type = "number")]
    pub port: u16,
    /// Whether the write tier is allowed at all.
    pub allow_write: bool,
    /// Whether the delete tier is allowed at all (trash only by default).
    pub allow_delete: bool,
    /// Whether `delete` may bypass the trash. Needs `allow_delete` too.
    pub allow_permanent_delete: bool,
    /// Ask in the application before every write/delete call.
    pub confirm_writes: bool,
    /// Addresses a client may reach. Empty means nothing is reachable.
    pub roots: Vec<String>,
    /// Add the panes' current folders to the roots while they are open.
    pub roots_follow_panes: bool,
    /// The largest file `read_file` will return, in bytes.
    #[ts(type = "number")]
    pub max_read_bytes: u64,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            // Off. An interface that lets a program outside the
            // application move the user's files is not something to
            // arrive switched on.
            enabled: false,
            port: 8712,
            allow_write: false,
            allow_delete: false,
            allow_permanent_delete: false,
            confirm_writes: true,
            roots: Vec::new(),
            roots_follow_panes: false,
            // The preview cap, for the same reason: a client must not be
            // able to pull a 10 GB file into its context.
            max_read_bytes: 2 * 1024 * 1024,
        }
    }
}

impl McpConfig {
    /// Whether `tier` is allowed by the configuration alone.
    ///
    /// Says nothing about scope or confirmation — those are separate
    /// gates and are checked separately, so a refusal can say which one
    /// it was.
    pub fn allows(&self, tier: McpTier) -> bool {
        match tier {
            McpTier::Read => true,
            McpTier::Write => self.allow_write,
            McpTier::Delete => self.allow_delete,
        }
    }
}

fn config_path() -> AppResult<std::path::PathBuf> {
    config::path("mcp.json")
}

fn load() -> McpConfig {
    let Ok(path) = config_path() else {
        return McpConfig::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => McpConfig::default(),
    }
}

/// The live configuration, loaded once and updated in place.
fn state() -> &'static Mutex<McpConfig> {
    static STATE: OnceLock<Mutex<McpConfig>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(load()))
}

/// A copy of the current configuration.
pub fn config() -> McpConfig {
    state().lock().expect("mcp config").clone()
}

/// Replaces the configuration and writes it out.
///
/// Validated here rather than trusted from the frontend: this is the
/// object that decides what a program outside the application may do,
/// and the frontend is not a security boundary (README § Security).
pub fn set_config(mut next: McpConfig) -> AppResult<McpConfig> {
    // Permanent deletion is meaningless — and dangerous to leave set —
    // without the tier that carries it.
    if !next.allow_delete {
        next.allow_permanent_delete = false;
    }
    next.max_read_bytes = next.max_read_bytes.clamp(1024, 64 * 1024 * 1024);
    next.roots = scope::normalize_roots(&next.roots);

    let path = config_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| {
            AppError::new(code::FS_IO)
                .op("mcp-config")
                .detail(format!("{}: {}", dir.display(), e))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(&next).map_err(|e| {
        AppError::new(code::INTERNAL_UNEXPECTED)
            .op("mcp-config")
            .detail(e)
    })?;
    std::fs::write(&path, bytes).map_err(|e| {
        AppError::new(code::FS_IO)
            .op("mcp-config")
            .detail(format!("{}: {}", path.display(), e))
    })?;

    *state().lock().expect("mcp config") = next.clone();
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_has_the_interface_off_and_nothing_reachable() {
        let c = McpConfig::default();
        assert!(!c.enabled, "the interface must not arrive switched on");
        assert!(c.roots.is_empty(), "no root means nothing is reachable");
        assert!(!c.allow_write);
        assert!(!c.allow_delete);
        assert!(!c.allow_permanent_delete);
        assert!(
            c.confirm_writes,
            "writes are confirmed until told otherwise"
        );
    }

    #[test]
    fn the_read_tier_needs_no_switch_but_the_others_do() {
        let mut c = McpConfig::default();
        assert!(c.allows(McpTier::Read));
        assert!(!c.allows(McpTier::Write));
        assert!(!c.allows(McpTier::Delete));

        c.allow_write = true;
        assert!(c.allows(McpTier::Write));
        assert!(!c.allows(McpTier::Delete), "one switch is not the other");
    }
}
