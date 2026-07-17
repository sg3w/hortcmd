// ============================================================
// Read directories, drives/mount points, home directory.
// ============================================================

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use ts_rs::TS;

// Note: ts-rs appends export_to to an implicit "<manifest>/bindings/",
// hence "../../src/ipc/bindings/" for the frontend directory in the repo root.
// u64 is deliberately declared as TS `number`: over Tauri's JSON IPC a
// JS number arrives (not bigint), matching the frontend code.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    #[ts(type = "number")]
    pub size: u64,
    // Modification time as Unix seconds (0 if unknown).
    #[ts(type = "number")]
    pub modified: u64,
    // Unix permission bits (st_mode); None on non-Unix platforms.
    #[ts(type = "number | null")]
    pub mode: Option<u32>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DirListing {
    // Canonical/absolute path of the read directory.
    pub path: String,
    pub entries: Vec<DirEntry>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Drive {
    pub name: String,
    pub mount: String,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub free: u64,
}

/// Reads the contents of a directory.
#[tauri::command]
pub fn list_dir(path: String) -> Result<DirListing, String> {
    let p = PathBuf::from(&path);
    let canonical = fs::canonicalize(&p).unwrap_or(p);

    let read = fs::read_dir(&canonical).map_err(|e| format!("{}: {}", canonical.display(), e))?;

    let mut entries = Vec::new();
    for item in read.flatten() {
        let meta = match item.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = item.file_name().to_string_lossy().into_owned();
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::MetadataExt;
            Some(meta.mode())
        };
        #[cfg(not(unix))]
        let mode = None;
        entries.push(DirEntry {
            name,
            is_dir: meta.is_dir(),
            is_symlink: meta.file_type().is_symlink(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            mode,
        });
    }

    Ok(DirListing {
        path: path_to_string(&canonical),
        entries,
    })
}

/// Lists drives or mount points along with their storage usage.
#[tauri::command]
pub fn list_drives() -> Vec<Drive> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();
    let mut drives: Vec<Drive> = disks
        .iter()
        .map(|d| {
            let mount = d.mount_point().to_string_lossy().into_owned();
            let name = if d.name().is_empty() {
                mount.clone()
            } else {
                d.name().to_string_lossy().into_owned()
            };
            Drive {
                name,
                mount,
                total: d.total_space(),
                free: d.available_space(),
            }
        })
        .collect();

    // Add home as a convenient shortcut.
    if let Some(home) = dirs::home_dir() {
        drives.push(Drive {
            name: "~".into(),
            mount: path_to_string(&home),
            total: 0,
            free: 0,
        });
    }
    drives
}

/// Home directory of the current user.
#[tauri::command]
pub fn home_dir() -> String {
    dirs::home_dir()
        .map(|p| path_to_string(&p))
        .unwrap_or_else(|| "/".into())
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
