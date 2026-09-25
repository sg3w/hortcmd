// ============================================================
// Recursive folder size.
//
// Runs on a background thread, streams intermediate totals over a
// Tauri `Channel` and can be cancelled by token (the frontend does
// so when the properties dialog closes).
//
// Symbolic links are never followed – that makes a loop (a → b → a)
// impossible. Unreadable entries are counted and skipped instead of
// aborting the walk.
// ============================================================

use super::cancel;
use crate::errors::{AppError, AppResult};
use crate::vfs::caps::{self, Capability};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use ts_rs::TS;

/// Running totals of a folder walk – payload of the progress channel
/// and of the final result.
#[derive(Clone, Copy, Default, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DirSizeStats {
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub files: u64,
    #[ts(type = "number")]
    pub dirs: u64,
    /// Entries that could not be read and were skipped.
    #[ts(type = "number")]
    pub errors: u64,
}

/// Result of `dir_size`.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DirSize {
    pub stats: DirSizeStats,
    /// True = cancelled early, `stats` is only a partial total.
    pub cancelled: bool,
}

/// Minimum gap between two progress events (the walk is much faster
/// than the UI needs to be).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

// ----- Cancellation (registry in `super::cancel`) -----

/// Cancel token namespace, so search and folder size cannot collide.
const KIND: &str = "dir_size";

/// Sets the cancel flag of a calculation. Also works before the task has
/// registered: the flag is stored, and the run then starts cancelled.
#[tauri::command]
pub fn cancel_dir_size(token: String) {
    cancel::cancel(KIND, &token);
}

/// Sums a folder tree iteratively (explicit stack instead of recursion,
/// so deep hierarchies cannot overflow the stack).
fn walk_size(root: PathBuf, cancel: &AtomicBool, on_progress: &Channel<DirSizeStats>) -> DirSize {
    let mut stats = DirSizeStats::default();
    let mut stack = vec![root];
    let mut last = Instant::now();

    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return DirSize {
                stats,
                cancelled: true,
            };
        }
        let read = match fs::read_dir(&dir) {
            Ok(read) => read,
            Err(_) => {
                stats.errors += 1;
                continue;
            }
        };

        for entry in read {
            if cancel.load(Ordering::Relaxed) {
                return DirSize {
                    stats,
                    cancelled: true,
                };
            }
            if last.elapsed() >= PROGRESS_INTERVAL {
                last = Instant::now();
                let _ = on_progress.send(stats);
            }

            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    stats.errors += 1;
                    continue;
                }
            };
            // `DirEntry::file_type`/`metadata` describe the link itself,
            // so a symlink is never mistaken for its target.
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    stats.errors += 1;
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stats.dirs += 1;
                stack.push(entry.path());
                continue;
            }
            match entry.metadata() {
                Ok(meta) => {
                    stats.files += 1;
                    stats.bytes += meta.len();
                }
                Err(_) => stats.errors += 1,
            }
        }
    }

    DirSize {
        stats,
        cancelled: false,
    }
}

/// Calculates the total size of a folder in the background and streams
/// intermediate totals over `on_progress`. `token` identifies the task
/// for `cancel_dir_size`.
#[tauri::command]
pub async fn dir_size(
    path: String,
    token: String,
    on_progress: Channel<DirSizeStats>,
) -> AppResult<DirSize> {
    // Same local traversal as the search (TICKET-031).
    caps::require(&path, Capability::RecursiveSize)?;
    let root = PathBuf::from(&path);
    // Register first, so every path out of here also unregisters again.
    let flag = cancel::register(KIND, &token);
    // Even the initial stat can block on an OS-mounted network volume.
    let result = tauri::async_runtime::spawn_blocking(move || {
        if root.is_dir() {
            Ok(walk_size(root, &flag, &on_progress))
        } else {
            Err(AppError::from(format!("{}: kein Verzeichnis", path)))
        }
    })
    .await
    .map_err(AppError::from)
    .and_then(|result| result);
    cancel::unregister(KIND, &token);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rc-size-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Walks without a live channel receiver – `send` errors are ignored,
    /// exactly like in the command.
    fn walk(root: &std::path::Path) -> DirSize {
        let channel = Channel::new(|_| Ok(()));
        walk_size(root.to_path_buf(), &AtomicBool::new(false), &channel)
    }

    #[test]
    fn empty_folder_is_zero() {
        let dir = tmp("empty");
        let res = walk(&dir);
        assert_eq!(res.stats.bytes, 0);
        assert_eq!(res.stats.files, 0);
        assert_eq!(res.stats.dirs, 0);
        assert!(!res.cancelled);
    }

    #[test]
    fn sums_files_recursively() {
        let dir = tmp("deep");
        fs::write(dir.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::create_dir_all(dir.join("sub/deeper")).unwrap();
        fs::write(dir.join("sub/b.bin"), vec![0u8; 20]).unwrap();
        fs::write(dir.join("sub/deeper/c.bin"), vec![0u8; 3]).unwrap();

        let res = walk(&dir);
        assert_eq!(res.stats.bytes, 123);
        assert_eq!(res.stats.files, 3);
        assert_eq!(res.stats.dirs, 2);
        assert_eq!(res.stats.errors, 0);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_terminates() {
        let dir = tmp("loop");
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/a.bin"), vec![0u8; 10]).unwrap();
        // sub/back → dir: following it would recurse forever.
        std::os::unix::fs::symlink(&dir, dir.join("sub/back")).unwrap();
        std::os::unix::fs::symlink(dir.join("sub/a.bin"), dir.join("link.bin")).unwrap();

        let res = walk(&dir);
        // Links contribute nothing and are not descended into.
        assert_eq!(res.stats.bytes, 10);
        assert_eq!(res.stats.files, 1);
        assert_eq!(res.stats.dirs, 1);
    }

    #[test]
    fn cancel_stops_the_walk() {
        let dir = tmp("cancel");
        fs::write(dir.join("a.bin"), vec![0u8; 10]).unwrap();
        let channel = Channel::new(|_| Ok(()));
        let res = walk_size(dir.clone(), &AtomicBool::new(true), &channel);
        assert!(res.cancelled);
        assert_eq!(res.stats.bytes, 0);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folder_is_counted_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tmp("unreadable");
        fs::write(dir.join("a.bin"), vec![0u8; 7]).unwrap();
        let locked = dir.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("hidden.bin"), vec![0u8; 999]).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let res = walk(&dir);
        // Restore, otherwise the temp dir cannot be cleaned up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root ignores the permission bits – then there is
        // nothing to skip and the hidden file is counted instead.
        if res.stats.errors > 0 {
            assert_eq!(res.stats.bytes, 7);
            assert_eq!(res.stats.files, 1);
        }
        assert_eq!(res.stats.dirs, 1);
    }

    #[test]
    fn command_cancels_through_the_shared_registry() {
        let flag = cancel::register(KIND, "t-1");
        assert!(!flag.load(Ordering::Relaxed));
        cancel_dir_size("t-1".into());
        assert!(flag.load(Ordering::Relaxed));
        cancel::unregister(KIND, "t-1");
    }
}
