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
use super::transfer::events::{ItemError, OpResult};
use super::transfer::registry::spawn_op;
use crate::errors::{code, AppError, AppResult};
use crate::vfs::caps::{self, Capability};
use crate::vfs::{self, Provider};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

/// Creates a directory (recursively).
#[tauri::command]
pub fn make_dir(path: String) -> AppResult<String> {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    caps::require(&path, Capability::Create)?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let undo = super::undo::prepare("mkdir", &path, &path)?;
    provider.create_dir_all(&resolved).map_err(AppError::from)?;
    if let Some(undo) = undo {
        undo.finish()?;
    }
    Ok(path)
}

/// Writes text into a file (e.g. an exported file list).
/// Without `overwrite`, an existing file is left untouched:
/// returning `Ok(false)` signals the name conflict to the frontend.
/// `Ok(true)` = written, `Err` = a real I/O error.
#[tauri::command]
pub fn write_text_file(path: String, contents: String, overwrite: bool) -> AppResult<bool> {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    caps::require(&path, Capability::Create)?;
    let (provider, resolved) = vfs::resolve(&path)?;
    let p = &resolved;
    if !overwrite && provider.exists(p) {
        return Ok(false);
    }
    provider
        .write(p, contents.as_bytes())
        .map_err(AppError::from)?;
    Ok(true)
}

/// Renames an entry (file or folder) within the same folder.
/// Refuses to overwrite an already existing target name.
#[tauri::command]
pub fn rename_entry(from: String, to: String) -> AppResult<()> {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    caps::require_pair(&from, Capability::Move, &to, Capability::CopyTo)?;
    let (provider, from_r) = vfs::resolve(&from)?;
    let (_, to_r) = vfs::resolve(&to)?;
    let from_p = &from_r;
    let to_p = &to_r;
    if !provider.exists(from_p) {
        return Err(AppError::from("Quelle existiert nicht"));
    }
    if provider.exists(to_p) {
        return Err(AppError::from("Zielname existiert bereits"));
    }
    let undo = super::undo::prepare("rename", &from, &to)?;
    provider.rename(from_p, to_p).map_err(AppError::from)?;
    if let Some(undo) = undo {
        undo.finish()?;
    }
    Ok(())
}

/// Distinguishes the temporary names a batch rename uses within one
/// process; the existence check in `unique_temp` closes the rest.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// Finds a free temporary name in the same folder (hidden file).
fn unique_temp(
    provider: &dyn Provider,
    base: &crate::vfs::StreamPath,
) -> std::io::Result<crate::vfs::StreamPath> {
    loop {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let cand = base.join(format!(".rcren-tmp-{}", n))?;
        if !provider.exists(&cand) {
            return Ok(cand);
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
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    if let Err(e) = caps::require(&dir, Capability::Move) {
        return OpResult {
            ok: 0,
            errors: vec![ItemError::new(&dir, e)],
        };
    }
    let (provider, base_str) = match vfs::resolve(&dir) {
        Ok(v) => v,
        Err(e) => {
            return OpResult {
                ok: 0,
                errors: vec![ItemError::new(&dir, e)],
            }
        }
    };
    rename_batch_on(&*provider, &base_str, renames, |from, to| {
        provider.rename(from, to)
    })
}

fn rename_batch_on(
    provider: &dyn Provider,
    base: &crate::vfs::StreamPath,
    renames: Vec<(String, String)>,
    rename: impl Fn(&crate::vfs::StreamPath, &crate::vfs::StreamPath) -> std::io::Result<()>,
) -> OpResult {
    let mut errors: Vec<ItemError> = Vec::new();

    // Source names (for the check "the target is itself being renamed").
    let sources: HashSet<&str> = renames.iter().map(|(f, _)| f.as_str()).collect();
    // Frequency of each target name (detect duplicate targets).
    let mut target_count: HashMap<&str, u32> = HashMap::new();
    for (_, to) in &renames {
        *target_count.entry(to.as_str()).or_insert(0) += 1;
    }

    // Filter out the valid pairs.
    let mut valid: Vec<(&str, &str, crate::vfs::StreamPath, crate::vfs::StreamPath)> = Vec::new();
    for (from, to) in &renames {
        if from == to {
            continue; // nichts zu tun
        }
        if to.is_empty() || to.contains('/') || to.contains('\\') {
            errors.push(ItemError::new(
                from,
                AppError::new(code::FS_INVALID_NAME).with("name", to),
            ));
            continue;
        }
        if target_count.get(to.as_str()).copied().unwrap_or(0) > 1 {
            errors.push(ItemError::new(
                from,
                AppError::new(code::VALIDATION_BAD_INPUT).with("name", to),
            ));
            continue;
        }
        let (source, target) = match base
            .join(from)
            .and_then(|source| Ok((source, base.join(to)?)))
        {
            Ok(paths) => paths,
            Err(error) => {
                errors.push(ItemError::new(from, error.into()));
                continue;
            }
        };
        if !provider.exists(&source) {
            errors.push(ItemError::new(from, AppError::new(code::FS_NOT_FOUND)));
            continue;
        }
        // Foreign collision: the target exists and is not itself being renamed.
        if provider.exists(&target) && !sources.contains(to.as_str()) {
            errors.push(ItemError::new(
                from,
                AppError::new(code::FS_EXISTS).with("name", to),
            ));
            continue;
        }
        valid.push((from.as_str(), to.as_str(), source, target));
    }

    let mut undo = HashMap::new();
    let group = format!(
        "rename-{}-{}",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    if provider.store_id() == "local" {
        for (from, _, source, target) in &valid {
            match super::undo::prepare_group(
                "rename",
                &source.to_string_lossy(),
                &target.to_string_lossy(),
                &group,
                true,
            ) {
                Ok(Some(pending)) => {
                    undo.insert(*from, pending);
                }
                Ok(None) => (),
                Err(error) => {
                    errors.push(ItemError::new(*from, error));
                    return OpResult { ok: 0, errors };
                }
            }
        }
    }

    // Phase 1: all sources to temporary names.
    let mut staged: Vec<(
        crate::vfs::StreamPath,
        &str,
        &str,
        &crate::vfs::StreamPath,
        &crate::vfs::StreamPath,
    )> = Vec::new(); // (temp, to, from)
    for (from, to, source, target) in &valid {
        let tmp = match unique_temp(provider, base) {
            Ok(tmp) => tmp,
            Err(error) => {
                errors.push(ItemError::new(*from, error.into()));
                continue;
            }
        };
        match rename(source, &tmp) {
            Ok(_) => staged.push((tmp, to, from, source, target)),
            Err(e) => errors.push(ItemError::new(*from, e.into())),
        }
    }

    // If any source remains in place, publishing a chain could destroy it.
    if staged.len() != valid.len() {
        for (tmp, _, from, source, _) in staged {
            restore_staged(provider, source, &tmp, from, &rename, &mut errors);
        }
        return OpResult { ok: 0, errors };
    }

    // Phase 2: temporary names to the target names.
    let mut ok = 0u32;
    for (tmp, to, from, source, target) in staged {
        let result = if provider.exists(target) {
            Err(AppError::new(code::FS_EXISTS).with("name", to))
        } else {
            rename(&tmp, target).map_err(AppError::from)
        };
        match result {
            Ok(_) => {
                ok += 1;
                if let Some(pending) = undo.remove(from) {
                    if let Err(error) = pending.finish() {
                        errors.push(ItemError::new(from, error));
                    }
                }
            }
            Err(e) => {
                errors.push(ItemError::new(from, e.with("name", to)));
                restore_staged(provider, source, &tmp, from, &rename, &mut errors);
            }
        }
    }

    if provider.store_id() == "local" {
        if let Err(error) = super::undo::seal(&group) {
            errors.push(ItemError::new(base.to_string_lossy().as_ref(), error));
        }
    }
    OpResult { ok, errors }
}

/// Keep the staged data and expose its location if restoring would replace a
/// published result or if the provider rejects rollback.
fn restore_staged(
    provider: &dyn Provider,
    original: &crate::vfs::StreamPath,
    tmp: &crate::vfs::StreamPath,
    from: &str,
    rename: &impl Fn(&crate::vfs::StreamPath, &crate::vfs::StreamPath) -> std::io::Result<()>,
    errors: &mut Vec<ItemError>,
) {
    let result = if provider.exists(original) {
        Err(AppError::new(code::FS_EXISTS).with("name", from))
    } else {
        rename(tmp, original).map_err(AppError::from)
    };
    if let Err(error) = result {
        errors.push(ItemError::new(
            tmp.to_string_lossy().as_ref(),
            error.with("operation", "batch-rollback"),
        ));
    }
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
    trash_entries_with(paths, |p| trash::delete(p).map_err(AppError::from))
}

fn trash_entries_with(
    paths: Vec<String>,
    mut dispose: impl FnMut(&str) -> AppResult<()>,
) -> OpResult {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    let mut ok = 0u32;
    let mut errors = Vec::new();

    for p in &paths {
        // The OS trash only holds local entries; a remote address must
        // never reach `trash::delete`, which would read it as a
        // relative path in the working directory (TICKET-031).
        if let Err(e) = caps::require(p, Capability::Trash) {
            errors.push(ItemError::new(p, e));
            continue;
        }
        // The library discards the original access error on macOS/Windows.
        // Inspect the entry itself (not the symlink target) before handing off.
        // This is evidence at this instant, not a classification of later failures.
        let result = std::fs::symlink_metadata(p)
            .map_err(AppError::from)
            .and_then(|_| dispose(p));
        match result {
            Ok(_) => ok += 1,
            Err(e) => errors.push(ItemError::new(p, e.op("trash"))),
        }
    }
    OpResult { ok, errors }
}

/// Deletes several entries (synchronously; files or directories recursively).
#[tauri::command]
pub fn delete_entries(paths: Vec<String>) -> OpResult {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    let mut ok = 0u32;
    let mut errors = Vec::new();

    for p in &paths {
        let res = caps::require(p, Capability::DeletePermanent)
            .and_then(|_| vfs::resolve(p))
            .and_then(|(provider, resolved)| provider.remove(&resolved).map_err(AppError::from));
        match res {
            Ok(_) => ok += 1,
            Err(e) => errors.push(ItemError::new(p, e)),
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
    fn failed_staging_preserves_every_source() {
        let d = tmp_dir();
        write(&d, "a.txt", "AAA");
        write(&d, "b.txt", "BBB");
        let result = super::rename_batch_on(
            &crate::vfs::local::LocalProvider,
            &crate::vfs::StreamPath::from(&d),
            vec![ren("a.txt", "b.txt"), ren("b.txt", "c.txt")],
            |from, to| {
                if from.file_name().unwrap() == "b.txt" {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "injected staging failure",
                    ));
                }
                fs::rename((from).native().unwrap(), (to).native().unwrap())
            },
        );
        assert_eq!(read(&d, "b.txt"), "BBB");
        assert_eq!(read(&d, "a.txt"), "AAA");
        assert_eq!(result.ok, 0);
        assert!(!result.errors.is_empty());
        assert!(!exists(&d, "c.txt"));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn failed_swap_publication_keeps_both_contents() {
        let d = tmp_dir();
        write(&d, "a.txt", "AAA");
        write(&d, "b.txt", "BBB");
        let result = super::rename_batch_on(
            &crate::vfs::local::LocalProvider,
            &crate::vfs::StreamPath::from(&d),
            vec![ren("a.txt", "b.txt"), ren("b.txt", "a.txt")],
            |from, to| {
                if to.file_name().unwrap() == "a.txt" {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "injected publication failure",
                    ));
                }
                fs::rename((from).native().unwrap(), (to).native().unwrap())
            },
        );
        assert_eq!(read(&d, "b.txt"), "AAA");
        assert_eq!(result.ok, 1);
        assert!(!result.errors.is_empty());
        let retained = fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".rcren-tmp-")
            })
            .unwrap();
        assert_eq!(fs::read_to_string(&retained).unwrap(), "BBB");
        assert!(result
            .errors
            .iter()
            .any(|e| e.item == retained.to_string_lossy()));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn target_created_during_staging_is_not_overwritten() {
        let d = tmp_dir();
        write(&d, "a.txt", "AAA");
        let result = super::rename_batch_on(
            &crate::vfs::local::LocalProvider,
            &crate::vfs::StreamPath::from(&d),
            vec![ren("a.txt", "x.txt")],
            |from, to| {
                fs::rename((from).native().unwrap(), (to).native().unwrap())?;
                if from.file_name().unwrap() == "a.txt" {
                    write(&d, "x.txt", "external");
                }
                Ok(())
            },
        );
        assert_eq!(read(&d, "x.txt"), "external");
        assert_eq!(read(&d, "a.txt"), "AAA");
        assert_eq!(result.ok, 0);
        assert!(!result.errors.is_empty());
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn unchanged_source_is_not_an_overwrite_permission() {
        let d = tmp_dir();
        write(&d, "a.txt", "AAA");
        write(&d, "b.txt", "BBB");
        let result = rename_batch(
            d.to_string_lossy().into(),
            vec![ren("a.txt", "a.txt"), ren("b.txt", "a.txt")],
        );
        assert_eq!(read(&d, "a.txt"), "AAA");
        assert_eq!(read(&d, "b.txt"), "BBB");
        assert_eq!(result.ok, 0);
        assert!(!result.errors.is_empty());
        fs::remove_dir_all(d).unwrap();
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

#[cfg(test)]
mod trash_error_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn reports_each_failure_and_preserves_sources_without_delete_fallback() {
        let root = std::env::temp_dir().join(format!("hortcmd-trash-{}", crate::net::unique_id()));
        std::fs::create_dir(&root).unwrap();
        let denied = root.join("denied");
        let failed = root.join("failed");
        let missing = root.join("missing");
        for path in [&denied, &failed] {
            std::fs::write(path, "keep me").unwrap();
        }
        let paths = [&denied, &missing, &failed]
            .map(|p| p.to_string_lossy().into_owned())
            .to_vec();
        let mut calls = Vec::new();
        let result = trash_entries_with(paths, |path| {
            calls.push(path.to_owned());
            if Path::new(path) == denied {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into())
            } else {
                Err(trash::Error::Unknown {
                    description: "opaque failure".into(),
                }
                .into())
            }
        });
        assert_eq!(result.ok, 0);
        assert_eq!(result.errors.len(), 3);
        assert_eq!(&*result.errors[0].error.code, code::FS_PERMISSION);
        assert_eq!(&*result.errors[1].error.code, code::FS_NOT_FOUND);
        assert_eq!(&*result.errors[2].error.code, code::TRASH_FAILED);
        assert_eq!(
            calls.len(),
            2,
            "missing entry never reaches the trash library"
        );
        for path in [&denied, &failed] {
            assert_eq!(std::fs::read_to_string(path).unwrap(), "keep me");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_an_existing_trashable_entry() {
        let root =
            std::env::temp_dir().join(format!("hortcmd-trash-link-{}", crate::net::unique_id()));
        std::fs::create_dir(&root).unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(root.join("absent"), &link).unwrap();
        let result = trash_entries_with(vec![link.to_string_lossy().into_owned()], |_| Ok(()));
        assert_eq!(result.ok, 1);
        assert!(result.errors.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
