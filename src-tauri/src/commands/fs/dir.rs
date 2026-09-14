// ============================================================
// Read directories, drives/mount points, home directory.
// ============================================================

use crate::errors::AppResult;
use serde::Serialize;
use std::path::Path;
use ts_rs::TS;

// Note: ts-rs appends export_to to an implicit "<manifest>/bindings/",
// hence "../../src/ipc/bindings/" for the frontend directory in the repo root.
// u64 is deliberately declared as TS `number`: over Tauri's JSON IPC a
// JS number arrives (not bigint), matching the frontend code.
#[derive(Clone, Serialize, TS)]
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
    // Entry states the frontend colors by (see commands/fs/attrs.rs).
    pub hidden: bool,
    pub readonly: bool,
    pub executable: bool,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DirListing {
    // Canonical/absolute path of the read directory.
    pub path: String,
    pub entries: Vec<DirEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Drive {
    pub name: String,
    pub mount: String,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub free: u64,
}

/// Reads the contents of a directory. The address is resolved through
/// the VFS registry, so the command works schema-agnostically; the
/// `local` provider handles ordinary file-system paths unchanged.
#[tauri::command]
pub async fn list_dir(path: String, fresh: Option<bool>) -> AppResult<DirListing> {
    if fresh.unwrap_or(false) {
        super::listings::read_fresh(path).await
    } else {
        super::listings::read(path).await
    }
}

/// Lists drives or mount points along with their storage usage.
#[tauri::command]
pub fn list_drives() -> Vec<Drive> {
    drive_snapshot().unwrap_or_default()
}

pub(super) fn drive_snapshot() -> Option<Vec<Drive>> {
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

    #[cfg(target_os = "macos")]
    drives.extend(
        super::drives_native::mounts()?
            .into_iter()
            .filter(|mount| mount.remote)
            .map(|mount| mount.drive),
    );
    if drives.is_empty() {
        return None;
    }
    drives.sort_by(|a, b| a.mount.cmp(&b.mount));
    drives.dedup_by(|a, b| a.mount == b.mount);
    // Add home as a convenient shortcut.
    if let Some(home) = dirs::home_dir() {
        if !drives
            .iter()
            .any(|drive| drive.mount == path_to_string(&home))
        {
            drives.push(Drive {
                name: "~".into(),
                mount: path_to_string(&home),
                total: 0,
                free: 0,
            });
        }
    }
    Some(drives)
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
