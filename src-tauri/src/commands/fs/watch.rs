// ============================================================
// Directory watching: observes the currently displayed
// folders and reports external changes via the "fs-changed" event.
// ============================================================

use notify::{recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

// A single watcher; rebuilt on every set_watched
// (the old one is automatically dropped/stopped on replacement).
static WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();
fn slot() -> &'static Mutex<Option<RecommendedWatcher>> {
    WATCHER.get_or_init(|| Mutex::new(None))
}

/// Watches exactly the given folders (non-recursive). On a change,
/// `fs-changed` is sent with the affected folder path.
#[tauri::command]
pub fn set_watched(app: AppHandle, paths: Vec<String>) {
    let app2 = app.clone();
    let mut watcher = match recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for p in event.paths {
                let dir = p.parent().unwrap_or(p.as_path());
                let _ = app2.emit("fs-changed", dir.to_string_lossy().into_owned());
            }
        }
    }) {
        Ok(w) => w,
        Err(_) => return,
    };

    for p in &paths {
        let _ = watcher.watch(Path::new(p), RecursiveMode::NonRecursive);
    }
    *slot().lock().unwrap() = Some(watcher);
}
