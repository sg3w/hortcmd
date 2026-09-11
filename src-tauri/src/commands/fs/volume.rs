// ============================================================
// Storage information for the volume a path lies on.
//
// Uses `sysinfo` – the same platform abstraction as `list_drives`
// (commands/fs/dir.rs) – so drive/mount differences between
// Windows, macOS, and Linux stay in one place.
// ============================================================

use crate::vfs::caps::{self, Capability};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Capacity and usage of a drive, partition, or mounted volume.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct VolumeInfo {
    pub name: String,
    pub mount: String,
    /// File system type, e.g. "apfs", "ntfs", "ext4".
    pub fs: String,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub used: u64,
    #[ts(type = "number")]
    pub free: u64,
    /// Share of the capacity, 0..=100 (0 when the capacity is unknown).
    pub used_pct: f64,
    pub free_pct: f64,
}

/// How well `mount` matches `path`: the number of matching path
/// components, or None if `path` does not lie under `mount`.
/// Compares component-wise, so "/Volumes/Data" does not match
/// the volume "/Volumes/Dat".
fn mount_rank(path: &Path, mount: &Path) -> Option<usize> {
    path.starts_with(mount).then(|| mount.components().count())
}

fn info_of(disk: &sysinfo::Disk) -> VolumeInfo {
    let mount = disk.mount_point().to_string_lossy().into_owned();
    let name = if disk.name().is_empty() {
        mount.clone()
    } else {
        disk.name().to_string_lossy().into_owned()
    };
    let total = disk.total_space();
    // "free" is what the current user may still use; everything else
    // counts as used – that is what file managers show.
    let free = disk.available_space().min(total);
    let used = total - free;
    let pct = |part: u64| {
        if total == 0 {
            0.0
        } else {
            part as f64 * 100.0 / total as f64
        }
    };

    VolumeInfo {
        name,
        mount,
        fs: disk.file_system().to_string_lossy().into_owned(),
        total,
        used,
        free,
        used_pct: pct(used),
        free_pct: pct(free),
    }
}

/// Finds the volume carrying `path`: the deepest matching mount point
/// wins ("/Volumes/Data" beats "/").
fn find(path: &Path) -> Option<VolumeInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter_map(|disk| mount_rank(path, disk.mount_point()).map(|rank| (rank, disk)))
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, disk)| info_of(disk))
}

/// Free bytes on the volume carrying `path`, or `None` when no mount
/// point matches. Used by failure-safe transfers (ticket 029) to refuse
/// staging that definitely cannot fit; an unknown volume stays `None`
/// so the transfer proceeds and a real write failure decides.
pub fn free_space_of(path: &Path) -> Option<u64> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    find(&target).map(|v| v.free)
}

/// Storage information about the volume `path` lies on; None if no
/// mount point matches. Runs in the background – reading the disk list
/// can be slow (network shares, spun-down drives).
#[tauri::command]
pub async fn volume_info(path: String) -> Result<Option<VolumeInfo>, String> {
    // `sysinfo` only knows locally mounted volumes; a remote address has
    // no disk of its own here (TICKET-031).
    caps::require(&path, Capability::VolumeInfo)?;
    tauri::async_runtime::spawn_blocking(move || {
        let raw = PathBuf::from(&path);
        // Resolve symlinks: only the real path lies under the right mount.
        let target = fs::canonicalize(&raw).unwrap_or(raw);
        find(&target)
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepest_mount_wins() {
        let path = Path::new("/Volumes/Data/projects/x");
        assert_eq!(mount_rank(path, Path::new("/")), Some(1));
        assert_eq!(mount_rank(path, Path::new("/Volumes")), Some(2));
        assert_eq!(mount_rank(path, Path::new("/Volumes/Data")), Some(3));
    }

    #[test]
    fn partial_name_is_no_match() {
        let path = Path::new("/Volumes/Data/x");
        // Prefix of the *string*, but not of the path.
        assert_eq!(mount_rank(path, Path::new("/Volumes/Dat")), None);
        assert_eq!(mount_rank(path, Path::new("/Users")), None);
    }

    #[test]
    fn volume_of_temp_dir_is_consistent() {
        // Whether a volume is found depends on the system; if one is
        // found, its numbers must add up.
        let Some(vol) = find(&std::env::temp_dir()) else {
            return;
        };
        assert_eq!(vol.used + vol.free, vol.total);
        assert!((0.0..=100.0).contains(&vol.used_pct));
        assert!((0.0..=100.0).contains(&vol.free_pct));
        assert!(!vol.mount.is_empty());
    }
}
