// ============================================================
// Permissions & properties of an entry: permissions (chmod),
// owner/group (display + chown), extended attributes, ACL, and
// checksums (MD5/SHA-1/SHA-256).
//
// Unix-specific parts (owner, xattr, ACL) are encapsulated behind
// `cfg(unix)`; on non-Unix platforms they return empty/None values,
// so the frontend gets the same structure everywhere.
// ============================================================

use super::attrs;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;
use ts_rs::TS;

/// An extended attribute: name, size of the raw value, and – if the
/// value is printable UTF-8 text – a truncated text preview.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct XattrItem {
    pub name: String,
    pub size: u32,
    /// UTF-8 preview (max. 256 characters) or None for binary data.
    pub value: Option<String>,
}

/// Collected properties of an entry (result of `file_props`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FileProps {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub modified: u64,
    /// Full st_mode (incl. file-type bits); None on non-Unix.
    #[ts(type = "number | null")]
    pub mode: Option<u32>,
    #[ts(type = "number | null")]
    pub uid: Option<u32>,
    #[ts(type = "number | null")]
    pub gid: Option<u32>,
    /// Resolved user name (or None if not resolvable/non-Unix).
    pub owner: Option<String>,
    pub group: Option<String>,
    pub xattrs: Vec<XattrItem>,
    /// ACL entries as text lines (empty if none/not supported).
    pub acl: Vec<String>,
    /// Whether the platform supports owner/xattr/ACL at all.
    pub unix: bool,
    /// Entry states the frontend colors by (see commands/fs/attrs.rs).
    /// Same fields as `DirEntry`, so both resolve the same color rules.
    pub hidden: bool,
    pub readonly: bool,
    pub executable: bool,
}

/// Three common checksums of a file's content.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Checksums {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

// ----- Platform-dependent helpers -----

#[cfg(unix)]
fn read_owner(meta: &fs::Metadata) -> (Option<u32>, Option<u32>, Option<String>, Option<String>) {
    use std::os::unix::fs::MetadataExt;
    let uid = meta.uid();
    let gid = meta.gid();
    let owner = uzers::get_user_by_uid(uid).map(|u| u.name().to_string_lossy().into_owned());
    let group = uzers::get_group_by_gid(gid).map(|g| g.name().to_string_lossy().into_owned());
    (Some(uid), Some(gid), owner, group)
}

#[cfg(not(unix))]
fn read_owner(_meta: &fs::Metadata) -> (Option<u32>, Option<u32>, Option<String>, Option<String>) {
    (None, None, None, None)
}

#[cfg(unix)]
fn read_xattrs(path: &Path) -> Vec<XattrItem> {
    let names = match xattr::list(path) {
        Ok(names) => names,
        Err(_) => return Vec::new(),
    };
    let mut items = Vec::new();
    for name in names {
        let raw = xattr::get(path, &name).ok().flatten().unwrap_or_default();
        let value = printable_preview(&raw);
        items.push(XattrItem {
            name: name.to_string_lossy().into_owned(),
            size: raw.len() as u32,
            value,
        });
    }
    items
}

#[cfg(not(unix))]
fn read_xattrs(_path: &Path) -> Vec<XattrItem> {
    Vec::new()
}

/// UTF-8 text preview of a raw value, if it is printable (otherwise None).
fn printable_preview(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t' && c != '\r') {
        return None;
    }
    let trimmed: String = s.chars().take(256).collect();
    Some(trimmed)
}

#[cfg(unix)]
fn read_acl(path: &Path) -> Vec<String> {
    match exacl::getfacl(path, None) {
        Ok(entries) => entries.iter().map(|e| e.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(not(unix))]
fn read_acl(_path: &Path) -> Vec<String> {
    Vec::new()
}

/// Reads all properties of an entry together.
#[tauri::command]
pub fn file_props(path: String) -> Result<FileProps, String> {
    let p = Path::new(&path);
    // symlink_metadata: describe the link itself, not the target.
    let meta = fs::symlink_metadata(p).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::MetadataExt;
        Some(meta.mode())
    };
    #[cfg(not(unix))]
    let mode = None;

    let (uid, gid, owner, group) = read_owner(&meta);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());

    Ok(FileProps {
        path: p.to_string_lossy().into_owned(),
        hidden: attrs::is_hidden(&meta, &name),
        readonly: attrs::is_readonly(&meta),
        executable: attrs::is_executable(&meta, &name),
        name,
        is_dir: meta.is_dir(),
        is_symlink: meta.file_type().is_symlink(),
        size: meta.len(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0),
        mode,
        uid,
        gid,
        owner,
        group,
        xattrs: read_xattrs(p),
        acl: read_acl(p),
        unix: cfg!(unix),
    })
}

/// Sets the access rights (chmod). `mode` are the lower permission bits
/// (0..=0o7777, incl. setuid/setgid/sticky).
#[tauri::command]
pub fn set_permissions(path: String, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode & 0o7777);
        fs::set_permissions(&path, perms).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Err("Rechte bearbeiten wird auf dieser Plattform nicht unterstützt".into())
    }
}

/// Changes owner and/or group (chown). `owner`/`group` may be a
/// numeric value ("501") or a name ("staff"); None leaves the
/// respective value unchanged. Usually requires root privileges.
#[tauri::command]
pub fn set_owner(path: String, owner: Option<String>, group: Option<String>) -> Result<(), String> {
    #[cfg(unix)]
    {
        let uid = match owner {
            Some(o) => Some(resolve_uid(&o)?),
            None => None,
        };
        let gid = match group {
            Some(g) => Some(resolve_gid(&g)?),
            None => None,
        };
        std::os::unix::fs::chown(&path, uid, gid).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, owner, group);
        Err("Besitzer ändern wird auf dieser Plattform nicht unterstützt".into())
    }
}

#[cfg(unix)]
fn resolve_uid(value: &str) -> Result<u32, String> {
    if let Ok(id) = value.parse::<u32>() {
        return Ok(id);
    }
    uzers::get_user_by_name(value)
        .map(|u| u.uid())
        .ok_or_else(|| format!("Unbekannter Benutzer: {}", value))
}

#[cfg(unix)]
fn resolve_gid(value: &str) -> Result<u32, String> {
    if let Ok(id) = value.parse::<u32>() {
        return Ok(id);
    }
    uzers::get_group_by_name(value)
        .map(|g| g.gid())
        .ok_or_else(|| format!("Unbekannte Gruppe: {}", value))
}

/// Computes MD5, SHA-1, and SHA-256 in a single read pass.
/// Runs synchronously; correspondingly slow for very large files.
#[tauri::command]
pub fn file_checksums(path: String) -> Result<Checksums, String> {
    use md5::Digest as _; // shared Digest trait (also for sha1/sha2)

    let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut md5 = md5::Md5::new();
    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];

    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        md5.update(&buf[..n]);
        sha1.update(&buf[..n]);
        sha256.update(&buf[..n]);
    }

    Ok(Checksums {
        md5: hex::encode(md5.finalize()),
        sha1: hex::encode(sha1.finalize()),
        sha256: hex::encode(sha256.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::{file_checksums, file_props};
    use std::fs;

    fn tmp_file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rc-props-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn checksums_of_empty_and_abc() {
        // Known reference values.
        let empty = tmp_file("empty", b"");
        let c = file_checksums(empty.to_string_lossy().into()).unwrap();
        assert_eq!(c.md5, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(c.sha1, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            c.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let abc = tmp_file("abc", b"abc");
        let c = file_checksums(abc.to_string_lossy().into()).unwrap();
        assert_eq!(c.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(c.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            c.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn props_reports_size_and_name() {
        let p = tmp_file("hello.txt", b"hello");
        let props = file_props(p.to_string_lossy().into()).unwrap();
        assert_eq!(props.name, "hello.txt");
        assert_eq!(props.size, 5);
        assert!(!props.is_dir);
    }
}
