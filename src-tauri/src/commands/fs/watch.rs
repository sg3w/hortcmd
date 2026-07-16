// ============================================================
// Verzeichnis-Überwachung: beobachtet die aktuell angezeigten
// Ordner und meldet externe Änderungen per Event "fs-changed".
// ============================================================

use notify::{recommended_watcher, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

// Ein einziger Watcher; wird bei jedem set_watched neu aufgebaut
// (der alte wird beim Ersetzen automatisch gedroppt/gestoppt).
static WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();
fn slot() -> &'static Mutex<Option<RecommendedWatcher>> {
    WATCHER.get_or_init(|| Mutex::new(None))
}

/// Überwacht genau die angegebenen Ordner (nicht rekursiv). Bei einer Änderung
/// wird `fs-changed` mit dem betroffenen Ordnerpfad gesendet.
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
