// ============================================================
// The file-operation commands (ticket 035).
//
// Thin adapters: each one validates the location against the capability
// contract (ticket 031), resolves the address through the VFS, and
// hands the work to the engine in `transfer/`. Nothing here decides how
// a transfer is planned, progressed or committed — that is the engine's
// job, and keeping this file free of it is what makes both readable.
//
// Command names, event names and payloads are unchanged by the split.
// ============================================================

use super::transfer::engine::run_transfer;
use super::transfer::events::OpResult;
use super::transfer::registry::spawn_op;
use crate::vfs::caps::{self, Capability};
use crate::vfs::{self, Provider};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

/// Creates a directory (recursively).
#[tauri::command]
pub fn make_dir(path: String) -> Result<String, String> {
    caps::require(&path, Capability::Create)?;
    let (provider, resolved) = vfs::resolve(&path)?;
    provider
        .create_dir_all(Path::new(&resolved))
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// Writes text into a file (e.g. an exported file list).
/// Without `overwrite`, an existing file is left untouched:
/// returning `Ok(false)` signals the name conflict to the frontend.
/// `Ok(true)` = written, `Err` = a real I/O error.
#[tauri::command]
pub fn write_text_file(path: String, contents: String, overwrite: bool) -> Result<bool, String> {
    caps::require(&path, Capability::Create)?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let p = Path::new(&resolved);
    if !overwrite && provider.exists(p) {
        return Ok(false);
    }
    provider
        .write(p, contents.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Renames an entry (file or folder) within the same folder.
/// Refuses to overwrite an already existing target name.
#[tauri::command]
pub fn rename_entry(from: String, to: String) -> Result<(), String> {
    caps::require_pair(&from, Capability::Move, &to, Capability::CopyTo)?;
    let (provider, from_r) = vfs::resolve(&from)?;
    let (_, to_r) = vfs::resolve(&to)?;
    let from_p = Path::new(&from_r);
    let to_p = Path::new(&to_r);
    if !provider.exists(from_p) {
        return Err("Quelle existiert nicht".into());
    }
    if provider.exists(to_p) {
        return Err("Zielname existiert bereits".into());
    }
    provider.rename(from_p, to_p).map_err(|e| e.to_string())
}

/// Distinguishes the temporary names a batch rename uses within one
/// process; the existence check in `unique_temp` closes the rest.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// Finds a free temporary name in the same folder (hidden file).
fn unique_temp(provider: &dyn Provider, base: &Path) -> PathBuf {
    loop {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let cand = base.join(format!(".rcren-tmp-{}", n));
        if !provider.exists(&cand) {
            return cand;
        }
    }
}

/// Renames several entries in `dir` (batch rename).
///
/// The new names are computed by the frontend; only the safe
/// execution happens here: unchanged pairs are skipped, conflicts (invalid
/// name, duplicate target, target taken by another, missing source) are reported. The
/// actual renaming runs in two phases (first to temporary names,
/// then to the target names) so that chains and name swaps work too.
#[tauri::command]
pub fn rename_batch(dir: String, renames: Vec<(String, String)>) -> OpResult {
    if let Err(e) = caps::require(&dir, Capability::Move) {
        return OpResult {
            ok: 0,
            errors: vec![e],
        };
    }
    let (provider, base_str) = match vfs::resolve(&dir) {
        Ok(v) => v,
        Err(e) => {
            return OpResult {
                ok: 0,
                errors: vec![e],
            }
        }
    };
    let base = Path::new(&base_str);
    let mut errors: Vec<String> = Vec::new();

    // Source names (for the check "the target is itself being renamed").
    let sources: HashSet<&str> = renames.iter().map(|(f, _)| f.as_str()).collect();
    // Frequency of each target name (detect duplicate targets).
    let mut target_count: HashMap<&str, u32> = HashMap::new();
    for (_, to) in &renames {
        *target_count.entry(to.as_str()).or_insert(0) += 1;
    }

    // Filter out the valid pairs.
    let mut valid: Vec<(&str, &str)> = Vec::new();
    for (from, to) in &renames {
        if from == to {
            continue; // nichts zu tun
        }
        if to.is_empty() || to.contains('/') || to.contains('\\') {
            errors.push(format!("{}: ungültiger Zielname", from));
            continue;
        }
        if target_count.get(to.as_str()).copied().unwrap_or(0) > 1 {
            errors.push(format!("{} → {}: Zielname mehrfach vergeben", from, to));
            continue;
        }
        if !provider.exists(&base.join(from)) {
            errors.push(format!("{}: Quelle nicht gefunden", from));
            continue;
        }
        // Foreign collision: the target exists and is not itself being renamed.
        if provider.exists(&base.join(to)) && !sources.contains(to.as_str()) {
            errors.push(format!("{} → {}: Ziel existiert bereits", from, to));
            continue;
        }
        valid.push((from.as_str(), to.as_str()));
    }

    // Phase 1: all sources to temporary names.
    let mut staged: Vec<(PathBuf, &str, &str)> = Vec::new(); // (temp, to, from)
    for (from, to) in &valid {
        let tmp = unique_temp(&*provider, base);
        match provider.rename(&base.join(from), &tmp) {
            Ok(_) => staged.push((tmp, to, from)),
            Err(e) => errors.push(format!("{}: {}", from, e)),
        }
    }

    // Phase 2: temporary names to the target names.
    let mut ok = 0u32;
    for (tmp, to, from) in staged {
        match provider.rename(&tmp, &base.join(to)) {
            Ok(_) => ok += 1,
            Err(e) => {
                // Rename back so no temporary name is left behind.
                let _ = provider.rename(&tmp, &base.join(from));
                errors.push(format!("{} → {}: {}", from, to, e));
            }
        }
    }

    OpResult { ok, errors }
}

/// Starts a copy operation in the background (returns immediately).
/// `limit` = bytes/s (0 = unlimited), `verify` = checksums after copying,
/// `safe` = stage and commit instead of writing to the final name
/// (ticket 029). `safe` is the value captured when the transfer was
/// queued, so a later settings change cannot alter a waiting operation.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn copy_entries(
    app: AppHandle,
    id: String,
    sources: Vec<String>,
    dest_dir: String,
    limit: u64,
    verify: bool,
    buf_kb: u64,
    threads: u32,
    safe: bool,
) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| {
        run_transfer(
            app,
            id2,
            "copy".into(),
            sources,
            dest_dir,
            false,
            cancel,
            pause,
            limit,
            verify,
            buf_kb,
            threads,
            safe,
        )
    });
}

/// Starts a move operation in the background (returns immediately).
/// `threads` is deliberately ignored (move stays sequential); `safe`
/// works as for `copy_entries` (ticket 029).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn move_entries(
    app: AppHandle,
    id: String,
    sources: Vec<String>,
    dest_dir: String,
    limit: u64,
    verify: bool,
    buf_kb: u64,
    threads: u32,
    safe: bool,
) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| {
        run_transfer(
            app,
            id2,
            "move".into(),
            sources,
            dest_dir,
            true,
            cancel,
            pause,
            limit,
            verify,
            buf_kb,
            threads,
            safe,
        )
    });
}

/// Moves several entries to the system trash (restorable).
#[tauri::command]
pub fn trash_entries(paths: Vec<String>) -> OpResult {
    let mut ok = 0u32;
    let mut errors = Vec::new();

    for p in &paths {
        // The OS trash only holds local entries; a remote address must
        // never reach `trash::delete`, which would read it as a
        // relative path in the working directory (TICKET-031).
        if let Err(e) = caps::require(p, Capability::Trash) {
            errors.push(format!("{}: {}", p, e));
            continue;
        }
        match trash::delete(p) {
            Ok(_) => ok += 1,
            Err(e) => errors.push(format!("{}: {}", p, e)),
        }
    }
    OpResult { ok, errors }
}

/// Deletes several entries (synchronously; files or directories recursively).
#[tauri::command]
pub fn delete_entries(paths: Vec<String>) -> OpResult {
    let mut ok = 0u32;
    let mut errors = Vec::new();

    for p in &paths {
        let res = caps::require(p, Capability::DeletePermanent)
            .and_then(|_| vfs::resolve(p))
            .and_then(|(provider, resolved)| {
                provider
                    .remove(Path::new(&resolved))
                    .map_err(|e| e.to_string())
            });
        match res {
            Ok(_) => ok += 1,
            Err(e) => errors.push(format!("{}: {}", p, e)),
        }
    }
    OpResult { ok, errors }
}

#[cfg(test)]
mod tests {
    use super::{rename_batch, write_text_file};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rc-rentest-{}-{}", t, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }
    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap()
    }
    fn exists(dir: &Path, name: &str) -> bool {
        dir.join(name).exists()
    }
    fn ren(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn simple_rename() {
        let d = tmp_dir();
        write(&d, "a.txt", "A");
        let r = rename_batch(d.to_string_lossy().into(), vec![ren("a.txt", "x.txt")]);
        assert_eq!(r.ok, 1);
        assert!(r.errors.is_empty());
        assert!(!exists(&d, "a.txt"));
        assert_eq!(read(&d, "x.txt"), "A");
    }

    #[test]
    fn swap_names() {
        // A name swap a<->b must work via the two-phase logic.
        let d = tmp_dir();
        write(&d, "a.txt", "AAA");
        write(&d, "b.txt", "BBB");
        let r = rename_batch(
            d.to_string_lossy().into(),
            vec![ren("a.txt", "b.txt"), ren("b.txt", "a.txt")],
        );
        assert_eq!(r.ok, 2, "errors: {:?}", r.errors);
        assert_eq!(read(&d, "a.txt"), "BBB");
        assert_eq!(read(&d, "b.txt"), "AAA");
    }

    #[test]
    fn chain_rename() {
        // Chain a->b, b->c (c new): result b=old A, c=old B.
        let d = tmp_dir();
        write(&d, "a.txt", "A");
        write(&d, "b.txt", "B");
        let r = rename_batch(
            d.to_string_lossy().into(),
            vec![ren("a.txt", "b.txt"), ren("b.txt", "c.txt")],
        );
        assert_eq!(r.ok, 2, "errors: {:?}", r.errors);
        assert!(!exists(&d, "a.txt"));
        assert_eq!(read(&d, "b.txt"), "A");
        assert_eq!(read(&d, "c.txt"), "B");
    }

    #[test]
    fn external_collision_is_skipped() {
        // The target "keep.txt" exists and is not itself renamed -> error,
        // the source stays untouched.
        let d = tmp_dir();
        write(&d, "a.txt", "A");
        write(&d, "keep.txt", "KEEP");
        let r = rename_batch(d.to_string_lossy().into(), vec![ren("a.txt", "keep.txt")]);
        assert_eq!(r.ok, 0);
        assert_eq!(r.errors.len(), 1);
        assert!(exists(&d, "a.txt"));
        assert_eq!(read(&d, "keep.txt"), "KEEP");
    }

    #[test]
    fn duplicate_targets_are_skipped() {
        let d = tmp_dir();
        write(&d, "a.txt", "A");
        write(&d, "b.txt", "B");
        let r = rename_batch(
            d.to_string_lossy().into(),
            vec![ren("a.txt", "same.txt"), ren("b.txt", "same.txt")],
        );
        assert_eq!(r.ok, 0);
        assert_eq!(r.errors.len(), 2);
        assert!(exists(&d, "a.txt"));
        assert!(exists(&d, "b.txt"));
        assert!(!exists(&d, "same.txt"));
    }

    #[test]
    fn write_text_file_respects_overwrite() {
        let d = tmp_dir();
        let p = d.join("list.txt").to_string_lossy().into_owned();
        assert_eq!(write_text_file(p.clone(), "first".into(), false), Ok(true));
        // Without overwrite: untouched -> Ok(false), the content stays.
        assert_eq!(
            write_text_file(p.clone(), "second".into(), false),
            Ok(false)
        );
        assert_eq!(read(&d, "list.txt"), "first");
        // With overwrite: written anew.
        assert_eq!(write_text_file(p.clone(), "second".into(), true), Ok(true));
        assert_eq!(read(&d, "list.txt"), "second");
    }
}
