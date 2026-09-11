// ============================================================
// Operation state: cancel flags, pause flags and the channels a
// collision question waits on (ticket 035).
//
// Every running operation — transfer, extract, pack — is addressed by
// its id. The three registries here are what `cancel_transfer`,
// `pause_transfer` and `resolve_collision` reach into, and `spawn_op`
// is what puts an entry in and takes it out again. Keeping them
// together means the whole lifetime of an operation's shared state is
// visible in one file.
//
// The maps are only ever held for the lookup itself: a transfer thread
// blocking on an answer must not also be holding the lock the answer
// needs.
// ============================================================

use super::events::CollisionReq;
use crate::vfs::Provider;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Resolution of a name conflict.
#[derive(Clone, Copy)]
pub(crate) enum Resolution {
    Overwrite,
    Rename,
    Skip,
}

pub(crate) type CancelMap = Mutex<HashMap<String, Arc<AtomicBool>>>;

pub(crate) static CANCELS: OnceLock<CancelMap> = OnceLock::new();

pub(crate) fn cancels() -> &'static CancelMap {
    CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Pause flags per operation (true = paused). The copy loop waits on them.
pub(crate) static PAUSES: OnceLock<CancelMap> = OnceLock::new();

pub(crate) fn pauses() -> &'static CancelMap {
    PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) type ResolverMap = Mutex<HashMap<String, mpsc::Sender<(Resolution, bool)>>>;

pub(crate) static RESOLVERS: OnceLock<ResolverMap> = OnceLock::new();

pub(crate) fn resolvers() -> &'static ResolverMap {
    RESOLVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) static REQ_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Asks the frontend for the conflict resolution and blocks until the answer.
/// Reacts to cancel (skip) so the thread never hangs.
pub(crate) fn ask_collision(
    app: &AppHandle,
    provider: &dyn Provider,
    transfer_id: &str,
    target: &Path,
    cancel: &Arc<AtomicBool>,
) -> (Resolution, bool) {
    let req_id = format!("c{}", REQ_COUNTER.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = mpsc::channel::<(Resolution, bool)>();
    resolvers().lock().unwrap().insert(req_id.clone(), tx);

    let _ = app.emit(
        "fs-collision",
        CollisionReq {
            transfer_id: transfer_id.to_string(),
            req_id: req_id.clone(),
            path: target.to_string_lossy().into_owned(),
            is_dir: provider.is_dir(target),
        },
    );

    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break (Resolution::Skip, false);
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(v) => break v,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break (Resolution::Skip, false),
        }
    };
    resolvers().lock().unwrap().remove(&req_id);
    result
}

/// Registers the cancel flag and starts a background operation.
/// Shared by transfers and archive operations.
pub(crate) fn spawn_op<F>(id: String, f: F)
where
    F: FnOnce(Arc<AtomicBool>, Arc<AtomicBool>) + Send + 'static,
{
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));
    cancels().lock().unwrap().insert(id.clone(), cancel.clone());
    pauses().lock().unwrap().insert(id.clone(), pause.clone());
    std::thread::spawn(move || {
        f(cancel, pause);
        cancels().lock().unwrap().remove(&id);
        pauses().lock().unwrap().remove(&id);
    });
}

/// Cancels a running transfer (sets its cancel flag).
#[tauri::command]
pub fn cancel_transfer(id: String) {
    if let Some(flag) = cancels().lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    // If paused: release the pause so the waiting thread sees the cancel.
    if let Some(flag) = pauses().lock().unwrap().get(&id) {
        flag.store(false, Ordering::Relaxed);
    }
}

/// Pauses or resumes a running transfer.
#[tauri::command]
pub fn pause_transfer(id: String, paused: bool) {
    if let Some(flag) = pauses().lock().unwrap().get(&id) {
        flag.store(paused, Ordering::Relaxed);
    }
}

/// Answer to a name conflict from the frontend.
#[tauri::command]
pub fn resolve_collision(req_id: String, action: String, apply_all: bool) {
    let res = match action.as_str() {
        "overwrite" => Resolution::Overwrite,
        "rename" => Resolution::Rename,
        _ => Resolution::Skip,
    };
    if let Some(tx) = resolvers().lock().unwrap().get(&req_id) {
        let _ = tx.send((res, apply_all));
    }
}
