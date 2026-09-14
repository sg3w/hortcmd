// ============================================================
// Basic Git integration: detects whether a folder lies in a Git
// repository and returns the status of the entries at this
// level (for color marking in the frontend). Uses the
// installed `git` CLI; without Git, `is_repo=false` is reported.
// ============================================================

use crate::vfs::caps::{self, Capability};
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
    /// Name (top level in the folder) → status code ("untracked" | "added" |
    /// "modified" | "deleted" | "renamed" | "conflict" | "ignored").
    pub entries: HashMap<String, String>,
}

/// Payload of the `git-support-ready` event: folder path + associated status,
/// so the frontend can assign the result to the correct tab (even
/// if the folder has changed in the meantime).
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

/// Runs `git -C <dir> <args…>` and returns stdout on success.
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

/// Maps a porcelain status code to a normalized status.
/// `sub` = the change is in a subfolder (→ folder "modified").
///
/// The two porcelain columns are the index (X) and the worktree (Y).
/// Conflict > rename > deletion > addition > modification for direct entries.
/// Descendant changes aggregate to modified (ignored stays ignored), never added.
/// Porcelain v1 folds dirty submodules into M, so they remain modified as well.
fn classify(code: &str, sub: bool) -> &'static str {
    if code == "!!" {
        return "ignored";
    }
    if sub {
        return "modified";
    }
    if code == "??" {
        return "untracked";
    }
    let x = code.as_bytes().first().copied().unwrap_or(b' ');
    let y = code.as_bytes().get(1).copied().unwrap_or(b' ');
    if x == b'U' || y == b'U' || code == "AA" || code == "DD" {
        "conflict"
    } else if x == b'R' || y == b'R' {
        "renamed"
    } else if x == b'D' || y == b'D' {
        "deleted"
    } else if x == b'A' {
        "added"
    } else {
        "modified"
    }
}

/// Determines the Git status for the entries of the folder `path` (blocking,
/// since via the `git` CLI). Runs only on a background thread now, see
/// [`git_status_watch`].
pub(crate) fn compute_git_status(path: &str) -> GitStatus {
    let dir = Path::new(path);

    // Determine the repo root; fails → no repo (or no git).
    let toplevel = match run_git(dir, &["rev-parse", "--show-toplevel"]) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return empty(false),
    };
    let top = PathBuf::from(&toplevel);

    let branch = run_git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "HEAD");

    // Folder path relative to the repo root (pathspec + prefix for matching).
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

    // Run status at the root → paths relative to the root; narrow to the
    // folder via pathspec.
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
        // Rename/Copy: the second path follows as its own NUL field → skip.
        if code.starts_with('R') || code.starts_with('C') {
            let _ = fields.next();
        }
        // Only consider entries below our folder.
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
        // Only replace an existing status if it was "ignored" (otherwise keep it).
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

/// Loads the Git status of `path` asynchronously in the background (Tokio task +
/// `spawn_blocking` for the `git` CLI) and returns the result via the event
/// `git-support-ready`. This blocks neither the folder opening nor
/// other IPC calls, even for very large repositories.
#[tauri::command]
pub async fn git_status_watch(app: AppHandle, path: String) {
    // The `git` CLI runs in a local working directory. A remote folder
    // reports "no Git support" rather than having its address handed to
    // a subprocess as a relative path (TICKET-031).
    if caps::require(&path, Capability::GitStatus).is_err() {
        let _ = app.emit(
            "git-support-ready",
            GitStatusEvent {
                path,
                status: empty(false),
            },
        );
        return;
    }
    let status = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || compute_git_status(&path)
    })
    .await
    .unwrap_or_else(|_| empty(false));
    let _ = app.emit("git-support-ready", GitStatusEvent { path, status });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_meaning_and_folder_aggregation() {
        for (code, expected) in [
            ("A ", "added"),
            ("AM", "added"),
            (" M", "modified"),
            ("M ", "modified"),
            ("MM", "modified"),
            ("??", "untracked"),
            ("UU", "conflict"),
            ("AA", "conflict"),
            ("DD", "conflict"),
            ("AU", "conflict"),
            ("AD", "deleted"),
            (" D", "deleted"),
            ("D ", "deleted"),
            ("R ", "renamed"),
            ("RM", "renamed"),
            ("RD", "renamed"),
            ("!!", "ignored"),
        ] {
            assert_eq!(classify(code, false), expected, "{code}");
            assert_eq!(
                classify(code, true),
                if code == "!!" { "ignored" } else { "modified" },
                "folder {code}"
            );
        }
        // Porcelain v1 reports changed/dirty submodules as M, never as added.
        assert_eq!(classify(" M", false), "modified");
    }

    #[test]
    fn real_repository_distinguishes_additions_from_staged_edits() {
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "hortcmd-git-colors-{}",
            format_args!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        ));
        std::fs::create_dir(&path).unwrap();
        let temp = Temp(path);
        let dir = temp.0.as_path();
        let git = |args: &[&str]| run_git(dir, args).expect("local test Git command");
        git(&["init"]);
        std::fs::write(dir.join("existing.txt"), "original\n").unwrap();
        git(&["add", "existing.txt"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ]);
        let status = || compute_git_status(dir.to_str().unwrap()).entries;
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        assert_eq!(status()["new.txt"], "untracked");
        git(&["add", "new.txt"]);
        assert_eq!(status()["new.txt"], "added");
        std::fs::write(dir.join("new.txt"), "new and edited\n").unwrap();
        assert_eq!(status()["new.txt"], "added");
        std::fs::write(dir.join("existing.txt"), "edited\n").unwrap();
        assert_eq!(status()["existing.txt"], "modified");
        git(&["add", "existing.txt"]);
        assert_eq!(status()["existing.txt"], "modified");
        std::fs::create_dir(dir.join("folder")).unwrap();
        std::fs::write(dir.join("folder/new.txt"), "new\n").unwrap();
        git(&["add", "folder"]);
        assert_eq!(status()["folder"], "modified");
        assert_eq!(
            compute_git_status(dir.join("folder").to_str().unwrap()).entries["new.txt"],
            "added"
        );
    }
}
