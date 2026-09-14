//! macOS cached mount-table metadata: MNT_NOWAIT never requests fresh disk I/O.
use super::dir::Drive;
use std::ffi::CStr;

pub(super) struct Mount {
    pub drive: Drive,
    pub remote: bool,
}
pub(super) fn mounts() -> Option<Vec<Mount>> {
    // getfsstat fills fixed-size statfs structures; bounds are checked before allocation.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if !(1..16384).contains(&count) {
        return None;
    }
    let capacity = count as usize + 8;
    let mut raw: Vec<libc::statfs> = vec![unsafe { std::mem::zeroed() }; capacity];
    let bytes = (capacity * std::mem::size_of::<libc::statfs>()) as libc::c_int;
    let count = unsafe { libc::getfsstat(raw.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    if count < 1 || count as usize >= capacity {
        return None;
    }
    let mut result = Vec::new();
    for entry in &raw[..count as usize] {
        if entry.f_flags & libc::MNT_DONTBROWSE as u32 != 0 {
            continue;
        }
        // Kernel paths are bounded and NUL-terminated, but still validate termination.
        let bytes: Vec<u8> = entry.f_mntonname.iter().map(|c| *c as u8).collect();
        let mount = CStr::from_bytes_until_nul(&bytes)
            .ok()?
            .to_str()
            .ok()?
            .to_owned();
        let name = std::path::Path::new(&mount)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&mount)
            .to_owned();
        result.push(Mount {
            remote: entry.f_flags & libc::MNT_LOCAL as u32 == 0,
            drive: Drive {
                name,
                mount,
                total: entry.f_blocks.saturating_mul(entry.f_bsize.into()),
                free: entry.f_bavail.saturating_mul(entry.f_bsize.into()),
            },
        });
    }
    result.sort_by(|a, b| a.drive.mount.cmp(&b.drive.mount));
    Some(result)
}
