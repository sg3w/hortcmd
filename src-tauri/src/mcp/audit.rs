// ============================================================
// What the interface was asked to do (TICKET-041).
//
// A ring buffer of every call: what was allowed, what was refused, and
// why. The refusals are the point. A misconfigured allowlist is silent
// by nature — the client simply gets nothing and the user sees no
// difference — and an entry saying "denied: outside the allowed roots"
// is the only thing that turns that into something anyone can fix.
//
// Nothing in here is a secret: no token, no credential, no file content.
// Addresses are the user's own and go in whole, because a log that says
// "a path was refused" helps nobody.
// ============================================================

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

/// How many calls are kept. Enough to explain a session, small enough
/// that it costs nothing to hold.
const CAPACITY: usize = 500;

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum McpOutcome {
    /// Ran, and the client got its answer.
    Allowed,
    /// Refused by a tier, the scope, or the token.
    Denied,
    /// Ran after the user approved it in the application.
    Confirmed,
    /// The user declined the confirmation. Nothing happened.
    Declined,
    /// Ran and failed — a missing file, a dead connection.
    Failed,
}

/// One line of the log.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpAuditEntry {
    /// Unix seconds.
    #[ts(type = "number")]
    pub at: u64,
    /// The client's self-reported name, or "unknown" — it is a label in
    /// a log, never a permission.
    pub client: String,
    /// The tool that was called.
    pub tool: String,
    /// The resolved target, when the call named one.
    pub target: Option<String>,
    pub outcome: McpOutcome,
    /// Why, for a refusal or a failure. Never a provider's raw message.
    pub detail: Option<String>,
}

fn log() -> &'static Mutex<VecDeque<McpAuditEntry>> {
    static LOG: OnceLock<Mutex<VecDeque<McpAuditEntry>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Records one call.
pub fn record(
    client: &str,
    tool: &str,
    target: Option<&str>,
    outcome: McpOutcome,
    detail: Option<String>,
) {
    let entry = McpAuditEntry {
        at: now(),
        client: if client.is_empty() {
            "unknown".to_string()
        } else {
            client.to_string()
        },
        tool: tool.to_string(),
        target: target.map(str::to_string),
        outcome,
        detail,
    };
    push(&mut log().lock().expect("mcp audit"), entry);
}

/// The buffer's own rule, apart from the global that holds it.
///
/// Tested on a local buffer: three tests sharing one process-wide log
/// measure each other rather than the code, and a ring buffer is exactly
/// the thing to get wrong at the boundary.
fn push(log: &mut VecDeque<McpAuditEntry>, entry: McpAuditEntry) {
    if log.len() == CAPACITY {
        log.pop_front();
    }
    log.push_back(entry);
}

/// The log, newest first.
pub fn entries() -> Vec<McpAuditEntry> {
    log()
        .lock()
        .expect("mcp audit")
        .iter()
        .rev()
        .cloned()
        .collect()
}

/// Empties the log — offered in the settings beside it.
pub fn clear() {
    log().lock().expect("mcp audit").clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tool: &str) -> McpAuditEntry {
        McpAuditEntry {
            at: 0,
            client: "test".into(),
            tool: tool.into(),
            target: None,
            outcome: McpOutcome::Allowed,
            detail: None,
        }
    }

    #[test]
    fn the_oldest_entry_falls_off_once_the_buffer_is_full() {
        let mut log = VecDeque::new();
        for i in 0..(CAPACITY + 10) {
            push(&mut log, entry(&format!("tool_{}", i)));
        }
        assert_eq!(log.len(), CAPACITY, "the buffer is bounded");
        assert_eq!(log.front().unwrap().tool, "tool_10");
        assert_eq!(log.back().unwrap().tool, format!("tool_{}", CAPACITY + 9));
    }

    /// The global log, exercised once. The two assertions that need it
    /// share a lock, because they share the buffer.
    fn exclusively<T>(body: impl FnOnce() -> T) -> T {
        static GUARD: Mutex<()> = Mutex::new(());
        let _held = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        let out = body();
        clear();
        out
    }

    /// A refusal is the entry that matters most: nothing else makes a
    /// misconfigured allowlist visible.
    #[test]
    fn a_refusal_keeps_its_reason_and_its_target() {
        exclusively(|| {
            record(
                "claude-desktop",
                "read_file",
                Some("/etc/passwd"),
                McpOutcome::Denied,
                Some("outside the allowed roots".into()),
            );
            let entry = &entries()[0];
            assert_eq!(entry.outcome, McpOutcome::Denied);
            assert_eq!(entry.target.as_deref(), Some("/etc/passwd"));
            assert_eq!(entry.detail.as_deref(), Some("outside the allowed roots"));
            assert!(entry.at > 0);
        });
    }

    #[test]
    fn a_client_that_gave_no_name_is_still_a_client() {
        exclusively(|| {
            record("", "list_dir", None, McpOutcome::Allowed, None);
            assert_eq!(entries()[0].client, "unknown");
        });
    }
}
