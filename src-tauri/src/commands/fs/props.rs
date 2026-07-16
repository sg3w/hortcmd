// ============================================================
// Rechte & Eigenschaften eines Eintrags: Berechtigungen (chmod),
// Besitzer/Gruppe (anzeigen + chown), Extended Attributes, ACL und
// Prüfsummen (MD5/SHA-1/SHA-256).
//
// Unix-spezifische Teile (Besitzer, xattr, ACL) sind hinter `cfg(unix)`
// gekapselt; auf Nicht-Unix-Plattformen liefern sie leere/None-Werte,
// damit das Frontend überall dieselbe Struktur erhält.
// ============================================================

use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;
use ts_rs::TS;

/// Ein Extended Attribute: Name, Größe des Rohwerts und – sofern der
/// Wert druckbarer UTF-8-Text ist – eine gekürzte Textvorschau.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct XattrItem {
    pub name: String,
    pub size: u32,
    /// UTF-8-Vorschau (max. 256 Zeichen) oder None bei Binärdaten.
    pub value: Option<String>,
}

/// Gesammelte Eigenschaften eines Eintrags (Ergebnis von `file_props`).
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
    /// Voller st_mode (inkl. Dateityp-Bits); None auf Nicht-Unix.
    #[ts(type = "number | null")]
    pub mode: Option<u32>,
    #[ts(type = "number | null")]
    pub uid: Option<u32>,
    #[ts(type = "number | null")]
    pub gid: Option<u32>,
    /// Aufgelöster Benutzername (oder None, wenn nicht auflösbar/Nicht-Unix).
    pub owner: Option<String>,
    pub group: Option<String>,
    pub xattrs: Vec<XattrItem>,
    /// ACL-Einträge als Textzeilen (leer, wenn keine/nicht unterstützt).
    pub acl: Vec<String>,
    /// Ob die Plattform Besitzer/xattr/ACL überhaupt unterstützt.
    pub unix: bool,
}

/// Drei gängige Prüfsummen eines Datei-Inhalts.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Checksums {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

// ----- Plattformabhängige Helfer -----

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

/// UTF-8-Textvorschau eines Rohwerts, falls er druckbar ist (sonst None).
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

/// Liest alle Eigenschaften eines Eintrags zusammen.
#[tauri::command]
pub fn file_props(path: String) -> Result<FileProps, String> {
    let p = Path::new(&path);
    // symlink_metadata: den Link selbst beschreiben, nicht das Ziel.
    let meta = fs::symlink_metadata(p).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::MetadataExt;
        Some(meta.mode())
    };
    #[cfg(not(unix))]
    let mode = None;

    let (uid, gid, owner, group) = read_owner(&meta);

    Ok(FileProps {
        path: p.to_string_lossy().into_owned(),
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
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

/// Setzt die Zugriffsrechte (chmod). `mode` sind die unteren Rechte-Bits
/// (0..=0o7777, inkl. setuid/setgid/sticky).
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

/// Ändert Besitzer und/oder Gruppe (chown). `owner`/`group` dürfen ein
/// numerischer Wert ("501") oder ein Name ("staff") sein; None lässt den
/// jeweiligen Wert unverändert. Erfordert i. d. R. Root-Rechte.
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

/// Berechnet MD5, SHA-1 und SHA-256 in einem einzigen Lese-Durchgang.
/// Läuft synchron; für sehr große Dateien entsprechend langsam.
#[tauri::command]
pub fn file_checksums(path: String) -> Result<Checksums, String> {
    use md5::Digest as _; // gemeinsames Digest-Trait (auch für sha1/sha2)

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
        // Bekannte Referenzwerte.
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
