// ============================================================
// Basis-Git-Integration: erkennt, ob ein Ordner in einem Git-
// Repository liegt, und liefert den Status der Einträge dieser
// Ebene (für farbliche Markierung im Frontend). Nutzt das
// installierte `git`-CLI; ohne Git wird `is_repo=false` gemeldet.
// ============================================================

use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    /// Name (oberste Ebene im Ordner) → Status-Code
    /// ("modified" | "new" | "deleted" | "renamed" | "conflict" | "ignored").
    pub entries: HashMap<String, String>,
}

/// Payload des Events `git-support-ready`: Ordnerpfad + zugehöriger Status,
/// damit das Frontend das Ergebnis dem richtigen Tab zuordnen kann (auch
/// wenn der Ordner inzwischen gewechselt wurde).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GitStatusEvent {
    pub path: String,
    pub status: GitStatus,
}

fn empty(is_repo: bool) -> GitStatus {
    GitStatus {
        is_repo,
        branch: None,
        entries: HashMap::new(),
    }
}

/// Führt `git -C <dir> <args…>` aus und liefert stdout bei Erfolg.
fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Ordnet einen Porcelain-Statuscode einem normalisierten Status zu.
/// `sub` = die Änderung liegt in einem Unterordner (→ Ordner „modified").
fn classify(code: &str, sub: bool) -> &'static str {
    if code == "!!" {
        return "ignored";
    }
    if sub {
        return "modified";
    }
    if code == "??" {
        return "new";
    }
    let x = code.as_bytes().first().copied().unwrap_or(b' ');
    let y = code.as_bytes().get(1).copied().unwrap_or(b' ');
    if x == b'U' || y == b'U' || code == "AA" || code == "DD" {
        "conflict"
    } else if x == b'R' || y == b'R' {
        "renamed"
    } else if x == b'A' || y == b'A' {
        "new"
    } else if x == b'D' || y == b'D' {
        "deleted"
    } else {
        "modified"
    }
}

/// Ermittelt den Git-Status für die Einträge des Ordners `path` (blockierend,
/// da über das `git`-CLI). Läuft nur noch auf einem Hintergrund-Thread, s.
/// [`git_status_watch`].
fn compute_git_status(path: &str) -> GitStatus {
    let dir = Path::new(path);

    // Repo-Wurzel bestimmen; schlägt fehl → kein Repo (oder kein git).
    let toplevel = match run_git(dir, &["rev-parse", "--show-toplevel"]) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return empty(false),
    };
    let top = PathBuf::from(&toplevel);

    let branch = run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD");

    // Ordnerpfad relativ zur Repo-Wurzel (Pathspec + Präfix zum Zuordnen).
    let dir_abs = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let rel = dir_abs
        .strip_prefix(&top)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let prefix = if rel.is_empty() {
        String::new()
    } else {
        format!("{}/", rel)
    };

    // Status an der Wurzel ausführen → Pfade relativ zur Wurzel; per Pathspec
    // auf den Ordner eingrenzen.
    let mut args = vec![
        "status",
        "--porcelain",
        "-z",
        "--untracked-files=all",
        "--ignored",
    ];
    if !rel.is_empty() {
        args.push("--");
        args.push(&rel);
    }
    let porcelain = run_git(&top, &args).unwrap_or_default();

    let mut entries: HashMap<String, String> = HashMap::new();
    let mut fields = porcelain.split('\0');
    while let Some(rec) = fields.next() {
        if rec.len() < 4 {
            continue;
        }
        let code = &rec[..2];
        let path_rel = &rec[3..];
        // Rename/Copy: der zweite Pfad folgt als eigenes NUL-Feld → überspringen.
        if code.starts_with('R') || code.starts_with('C') {
            let _ = fields.next();
        }
        // Nur Einträge unterhalb unseres Ordners betrachten.
        let rest = if prefix.is_empty() {
            Some(path_rel)
        } else {
            path_rel.strip_prefix(&prefix)
        };
        let rest = match rest {
            Some(r) if !r.is_empty() => r.trim_end_matches('/'),
            _ => continue,
        };
        let sub = rest.contains('/');
        let name = rest.split('/').next().unwrap_or(rest).to_string();
        if name.is_empty() {
            continue;
        }
        let status = classify(code, sub);
        // Vorhandenen Status nur ersetzen, wenn er „ignored" war (sonst behalten).
        match entries.get(name.as_str()) {
            Some(cur) if cur != "ignored" => {}
            _ => {
                entries.insert(name, status.to_string());
            }
        }
    }

    GitStatus {
        is_repo: true,
        branch,
        entries,
    }
}

/// Lädt den Git-Status von `path` asynchron im Hintergrund (Tokio-Task +
/// `spawn_blocking` fürs `git`-CLI) und liefert das Ergebnis per Event
/// `git-support-ready`. Blockiert dadurch weder das Öffnen des Ordners noch
/// andere IPC-Aufrufe, auch nicht bei sehr großen Repositories.
#[tauri::command]
pub async fn git_status_watch(app: AppHandle, path: String) {
    let status = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || compute_git_status(&path)
    })
    .await
    .unwrap_or_else(|_| empty(false));
    let _ = app.emit("git-support-ready", GitStatusEvent { path, status });
}
