// ============================================================
// Asking the user (TICKET-041).
//
// A write or a delete arriving from outside the application is not
// something to do quietly. The request thread raises a dialog in the
// running window and waits for an answer — which is exactly as
// dangerous as it sounds if it is done carelessly, so:
//
//   * the wait happens on the **request** thread, never the UI thread.
//     The interface serves each connection on its own task, so a pending
//     question blocks that one client and nothing else — the read tier
//     keeps answering, and so does the application;
//   * it is **bounded**. A client that asks while nobody is at the
//     machine gets a refusal after two minutes rather than holding a
//     thread until the process ends;
//   * there is no default-yes anywhere in it. A timeout, a closed
//     window, a missing window: all of them are "no", and all of them
//     say which.
//
// The per-session "allow this tool for this client" option is here too,
// keyed by client *and* tool: agreeing once to `write_file` is not
// agreeing to `delete`, and it lasts as long as the process does, not
// longer.
// ============================================================

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

/// How long a question waits before it answers itself with "no".
const TIMEOUT: Duration = Duration::from_secs(120);

/// What the user said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Approved,
    Declined,
    /// Nobody could be asked — no window, or the question timed out.
    Unavailable,
}

/// The question, as the window receives it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpConfirmRequest {
    /// Matches the answer back to the waiting request.
    pub id: String,
    /// The client's self-reported name — a label, never a permission.
    pub client: String,
    pub tool: String,
    /// What this call will do, in the words the tool itself chose.
    pub summary: String,
}

#[derive(Default)]
struct Pending {
    /// Questions waiting for an answer, and the answers that arrived.
    answers: HashMap<String, (String, Option<bool>)>,
    /// `client\u{0}tool` pairs the user has waved through for this run.
    remembered: HashSet<String>,
}

fn pending() -> &'static (Mutex<Pending>, Condvar) {
    static PENDING: OnceLock<(Mutex<Pending>, Condvar)> = OnceLock::new();
    PENDING.get_or_init(|| (Mutex::new(Pending::default()), Condvar::new()))
}

/// The window to ask in, once the application has one.
fn app() -> &'static Mutex<Option<AppHandle>> {
    static APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
    APP.get_or_init(|| Mutex::new(None))
}

/// Called from the application's setup.
pub fn attach(handle: AppHandle) {
    *app().lock().expect("mcp confirm app") = Some(handle);
}

fn remembered_key(client: &str, tool: &str) -> String {
    format!("{}\u{0}{}", client, tool)
}

/// Asks, and waits for the answer.
pub fn ask(session: &str, client: &str, tool: &str, summary: &str) -> Answer {
    let key = remembered_key(session, tool);
    {
        let (lock, _) = pending();
        if lock.lock().expect("mcp confirm").remembered.contains(&key) {
            return Answer::Approved;
        }
    }

    let handle = match app().lock().expect("mcp confirm app").clone() {
        Some(h) => h,
        // No window means nobody to ask, which is not a reason to
        // proceed. It is a reason to refuse and say so.
        None => return Answer::Unavailable,
    };

    let id = crate::net::unique_id();
    let request = McpConfirmRequest {
        id: id.clone(),
        client: client.to_string(),
        tool: tool.to_string(),
        summary: summary.to_string(),
    };

    let (lock, cvar) = pending();
    lock.lock()
        .expect("mcp confirm")
        .answers
        .insert(id.clone(), (key, None));

    if handle.emit("mcp-confirm", &request).is_err() {
        lock.lock().expect("mcp confirm").answers.remove(&id);
        return Answer::Unavailable;
    }

    struct CloseRequest {
        handle: AppHandle,
        id: String,
    }
    impl Drop for CloseRequest {
        fn drop(&mut self) {
            let _ = self.handle.emit("mcp-confirm-closed", &self.id);
        }
    }
    let _close = CloseRequest {
        handle,
        id: id.clone(),
    };

    let mut guard = lock.lock().expect("mcp confirm");
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match guard.answers.get(&id) {
            Some((_, Some(true))) => {
                guard.answers.remove(&id);
                return Answer::Approved;
            }
            Some((_, Some(false))) => {
                guard.answers.remove(&id);
                return Answer::Declined;
            }
            // Still waiting, or the entry is gone because the window
            // went away while we were asking.
            Some((_, None)) => {}
            None => return Answer::Unavailable,
        }
        if super::execution::cancel_flag().load(std::sync::atomic::Ordering::Relaxed) {
            guard.answers.remove(&id);
            return Answer::Unavailable;
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            guard.answers.remove(&id);
            // A question nobody answered is a "no". The alternative is a
            // client that waits for a person who went home.
            return Answer::Unavailable;
        }
        let (next, _timeout) = cvar
            .wait_timeout(guard, left.min(Duration::from_millis(200)))
            .expect("mcp confirm wait");
        guard = next;
    }
}

/// The window's answer.
pub fn answer(id: &str, approved: bool, remember: bool) {
    let (lock, cvar) = pending();
    let mut guard = lock.lock().expect("mcp confirm");
    let Some((key, slot)) = guard.answers.get_mut(id) else {
        return;
    };
    if slot.is_some() {
        return;
    }
    *slot = Some(approved);
    let key = key.clone();
    if approved && remember {
        guard.remembered.insert(key);
    }
    drop(guard);
    cvar.notify_all();
}

/// Forgets every "allow this tool for this client" of this run.
pub fn forget_remembered() {
    let (lock, cvar) = pending();
    let mut state = lock.lock().expect("mcp confirm");
    state.remembered.clear();
    state.answers.clear();
    cvar.notify_all();
}

/// Revoke only this server-generated session's grants and pending questions.
pub fn forget_session(session: &str) {
    let prefix = format!("{session}\u{0}");
    let (lock, cvar) = pending();
    let mut state = lock.lock().expect("mcp confirm");
    state.remembered.retain(|key| !key.starts_with(&prefix));
    state
        .answers
        .retain(|_, (key, _)| !key.starts_with(&prefix));
    cvar.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_answers_cannot_grant_permission() {
        answer("unknown", true, true);
        assert!(!pending().0.lock().unwrap().remembered.contains("unknown"));
        assert_eq!(
            ask("session", "client", "write_file", "write"),
            Answer::Unavailable
        );
    }

    #[test]
    fn remembered_keys_use_session_identity() {
        assert_ne!(
            remembered_key("session-a", "write_file"),
            remembered_key("session-b", "write_file")
        );
        assert_ne!(
            remembered_key("session-a", "write_file"),
            remembered_key("session-a", "delete")
        );
    }
}
