// ============================================================
// Selection files (TICKET-059): persisting the marked entries of a
// tab as a versioned UTF-8 JSON document.
//
// Reading, writing and — above all — *validating* the format live
// here in the backend, so a hand-edited or foreign file can never
// reach the panes as a list of paths. The frontend only ever sees a
// `SelectionFile` that has already passed every rule below.
//
// Deliberately not compatible with Total Commander's line-based
// `.lst` files: plain lines cannot carry names containing tabs or
// line breaks without an escaping scheme of their own, and claiming
// compatibility with an unspecified format would be a promise we
// cannot keep.
// ============================================================

use crate::errors::{code, AppError, AppResult, ErrorAction};
use crate::vfs;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use ts_rs::TS;

/// Format version written by this build; `load_selection` rejects
/// anything else rather than guessing at an unknown layout.
pub const SELECTION_VERSION: u32 = 1;

/// Upper limits. They bound both the read buffer and the amount of
/// work a single load can put on the pane.
const MAX_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB
const MAX_ENTRIES: usize = 100_000;

/// One remembered entry: the plain child name plus its kind, so a
/// load can refuse to mark a folder that used to be a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SelEntry {
    pub name: String,
    pub is_dir: bool,
}

/// The file as a whole. `location` is the canonical address of the
/// folder the selection was taken in (including provider identity and,
/// inside an archive, the inner path) — never any credentials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SelectionFile {
    pub version: u32,
    pub location: String,
    pub entries: Vec<SelEntry>,
}

/// A single entry name must address a *direct child* of one folder.
/// Everything that could widen that — separators, traversal, a
/// provider address — is rejected instead of being sanitised, so a
/// crafted file cannot smuggle another location past the caller.
fn check_name(name: &str) -> AppResult<()> {
    // One code for every shape of an unusable entry name; the offending
    // name travels as a parameter so the frontend can quote it.
    let bad = |name: &str| {
        Err(AppError::new(code::FS_INVALID_NAME)
            .with("name", name)
            .action(ErrorAction::Report))
    };
    if name.is_empty() || name == "." || name == ".." {
        return bad(name);
    }
    if name.contains('/') || name.contains('\\') || name.contains("://") {
        return bad(name);
    }
    // NUL can never occur in a real file name and would truncate the
    // name for any C-level API behind a provider. Tabs and line breaks,
    // on the other hand, are legal on POSIX — carrying them losslessly
    // is exactly why this format is JSON and not one name per line.
    if name.contains('\0') {
        return bad(name);
    }
    Ok(())
}

/// Validates a parsed document and folds duplicates together.
/// Two entries count as duplicates when name *and* kind match; the
/// same name with two different kinds is a contradiction and fails.
fn normalize(file: SelectionFile) -> AppResult<SelectionFile> {
    if file.version != SELECTION_VERSION {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT)
            .with("found", file.version)
            .with("expected", SELECTION_VERSION)
            .action(ErrorAction::Report));
    }
    if file.location.trim().is_empty() {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).action(ErrorAction::Report));
    }
    if file.entries.len() > MAX_ENTRIES {
        return Err(AppError::new(code::FS_TOO_LARGE)
            .with("found", file.entries.len())
            .with("limit", MAX_ENTRIES)
            .action(ErrorAction::Report));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut entries = Vec::with_capacity(file.entries.len());
    for e in file.entries {
        check_name(&e.name)?;
        if !seen.insert(e.name.clone()) {
            // Already known – identical kind folds away, a different
            // kind means the file contradicts itself.
            if entries
                .iter()
                .any(|k: &SelEntry| k.name == e.name && k.is_dir != e.is_dir)
            {
                return Err(AppError::from(format!(
                    "Widersprüchlicher Eintrag: {}",
                    e.name
                )));
            }
            continue;
        }
        entries.push(e);
    }
    Ok(SelectionFile {
        version: SELECTION_VERSION,
        location: file.location,
        entries,
    })
}

/// Writes the selection as JSON. Without `overwrite` an existing file
/// is left alone and `false` reports the name conflict — the same
/// contract as `write_text_file`, so the caller reuses the existing
/// overwrite confirmation.
#[tauri::command]
pub fn save_selection(
    path: String,
    location: String,
    entries: Vec<SelEntry>,
    overwrite: bool,
) -> AppResult<bool> {
    let file = normalize(SelectionFile {
        version: SELECTION_VERSION,
        location,
        entries,
    })?;
    let json = serde_json::to_string_pretty(&file).map_err(AppError::from)?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let p = Path::new(&resolved);
    if !overwrite && provider.exists(p) {
        return Ok(false);
    }
    provider.write(p, json.as_bytes()).map_err(AppError::from)?;
    Ok(true)
}

/// Reads and validates a selection file. Over-long files fail before
/// parsing: the reader stops one byte past the limit, so a huge or
/// endless stream cannot be pulled into memory.
#[tauri::command]
pub fn load_selection(path: String) -> AppResult<SelectionFile> {
    let (provider, resolved) = vfs::resolve(&path)?;
    let p = Path::new(&resolved);
    if let Ok(st) = provider.stat(p) {
        if st.len > MAX_BYTES {
            return Err(AppError::new(code::FS_TOO_LARGE)
                .with("limit", MAX_BYTES / 1048576)
                .action(ErrorAction::Report));
        }
    }
    let mut buf = Vec::new();
    provider
        .open_read(p)
        .map_err(AppError::from)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(AppError::from)?;
    if buf.len() as u64 > MAX_BYTES {
        return Err(AppError::new(code::FS_TOO_LARGE)
            .with("limit", MAX_BYTES / 1048576)
            .action(ErrorAction::Report));
    }
    let text = String::from_utf8(buf).map_err(|e| {
        AppError::new(code::VALIDATION_BAD_INPUT)
            .action(ErrorAction::Report)
            .detail(e)
    })?;
    let parsed: SelectionFile = serde_json::from_str(&text).map_err(|e| {
        AppError::new(code::VALIDATION_BAD_INPUT)
            .action(ErrorAction::Report)
            .detail(e)
    })?;
    normalize(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rc-seltest-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(name: &str, is_dir: bool) -> SelEntry {
        SelEntry {
            name: name.into(),
            is_dir,
        }
    }

    fn file(entries: Vec<SelEntry>) -> SelectionFile {
        SelectionFile {
            version: SELECTION_VERSION,
            location: "/tmp/x".into(),
            entries,
        }
    }

    #[test]
    fn roundtrip_keeps_unicode_tabs_and_newlines() {
        let dir = tmp_dir();
        let target = dir.join("auswahl.json");
        // Names a line-based format could not carry losslessly.
        let entries = vec![
            entry("Grüße \u{1F600}.txt", false),
            entry("mit\ttab.txt", false),
            entry("mit\nzeilenumbruch.txt", false),
            entry("Ordner", true),
        ];
        let written = save_selection(
            target.to_string_lossy().into_owned(),
            "/tmp/quelle".into(),
            entries.clone(),
            false,
        )
        .unwrap();
        assert!(written);
        let back = load_selection(target.to_string_lossy().into_owned()).unwrap();
        assert_eq!(back.version, SELECTION_VERSION);
        assert_eq!(back.location, "/tmp/quelle");
        assert_eq!(back.entries, entries);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_selection_is_a_valid_file() {
        let dir = tmp_dir();
        let target = dir.join("leer.json");
        assert!(save_selection(
            target.to_string_lossy().into_owned(),
            "/tmp/quelle".into(),
            vec![],
            false,
        )
        .unwrap());
        let back = load_selection(target.to_string_lossy().into_owned()).unwrap();
        assert!(back.entries.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn existing_file_is_not_overwritten_without_the_flag() {
        let dir = tmp_dir();
        let target = dir.join("da.json");
        let p = target.to_string_lossy().into_owned();
        assert!(save_selection(p.clone(), "/a".into(), vec![entry("a", false)], false).unwrap());
        // Second write without `overwrite` reports the conflict and
        // leaves the first content untouched.
        assert!(!save_selection(p.clone(), "/b".into(), vec![entry("b", false)], false).unwrap());
        assert_eq!(load_selection(p.clone()).unwrap().location, "/a");
        assert!(save_selection(p.clone(), "/b".into(), vec![entry("b", false)], true).unwrap());
        assert_eq!(load_selection(p).unwrap().location, "/b");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wrong_version_is_rejected() {
        let mut f = file(vec![entry("a", false)]);
        f.version = 2;
        assert!(normalize(f).unwrap_err().is(code::VALIDATION_BAD_INPUT));
    }

    #[test]
    fn traversal_and_separators_are_rejected() {
        for bad in [
            "..",
            ".",
            "../x",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "sftp://h/x",
        ] {
            let err = normalize(file(vec![entry(bad, false)])).unwrap_err();
            assert!(
                err.is(code::FS_INVALID_NAME),
                "{} sollte abgelehnt werden, war: {:?}",
                bad,
                err
            );
            // The offending name travels as a parameter, so the
            // frontend can quote it in either language.
            assert_eq!(err.params.get("name").map(String::as_str), Some(bad));
        }
    }

    #[test]
    fn empty_and_nul_names_are_rejected() {
        assert!(normalize(file(vec![entry("", false)])).is_err());
        assert!(normalize(file(vec![entry("a\u{0}b", false)])).is_err());
        // …but a tab or a line break is a legal POSIX name and survives.
        assert!(normalize(file(vec![entry("a\tb\nc", false)])).is_ok());
    }

    #[test]
    fn duplicates_are_folded_and_contradictions_fail() {
        let ok = normalize(file(vec![
            entry("a.txt", false),
            entry("a.txt", false),
            entry("b", true),
        ]))
        .unwrap();
        assert_eq!(ok.entries, vec![entry("a.txt", false), entry("b", true)]);
        assert!(normalize(file(vec![entry("a", false), entry("a", true)])).is_err());
    }

    #[test]
    fn entry_limit_is_enforced() {
        let many = (0..=MAX_ENTRIES)
            .map(|i| entry(&format!("f{}", i), false))
            .collect();
        assert!(normalize(file(many)).unwrap_err().is(code::FS_TOO_LARGE));
    }

    #[test]
    fn missing_location_is_rejected() {
        let mut f = file(vec![]);
        f.location = "  ".into();
        assert!(normalize(f).is_err());
    }

    #[test]
    fn broken_json_and_non_utf8_are_reported() {
        let dir = tmp_dir();
        let broken = dir.join("kaputt.json");
        fs::write(&broken, b"{ nicht json").unwrap();
        assert!(load_selection(broken.to_string_lossy().into_owned())
            .unwrap_err()
            .is(code::VALIDATION_BAD_INPUT));
        let binary = dir.join("binaer.json");
        fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(load_selection(binary.to_string_lossy().into_owned())
            .unwrap_err()
            .is(code::VALIDATION_BAD_INPUT));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn oversized_file_is_refused_before_parsing() {
        let dir = tmp_dir();
        let big = dir.join("gross.json");
        fs::write(&big, vec![b'x'; (MAX_BYTES + 16) as usize]).unwrap();
        assert!(load_selection(big.to_string_lossy().into_owned())
            .unwrap_err()
            .is(code::FS_TOO_LARGE));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_an_invalid_name_writes_nothing() {
        let dir = tmp_dir();
        let target = dir.join("nie.json");
        assert!(save_selection(
            target.to_string_lossy().into_owned(),
            "/a".into(),
            vec![entry("../escape", false)],
            false,
        )
        .is_err());
        assert!(!target.exists());
        fs::remove_dir_all(&dir).ok();
    }
}
