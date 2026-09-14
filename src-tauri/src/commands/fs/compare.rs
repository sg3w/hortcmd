// ============================================================
// Directory comparison & synchronization.
//   compare_dirs – compare two trees recursively (by size + time)
//   sync_copy    – copy selected files from one side to the other
// The comparison returns a status per file; the copying is done
// by the backend (target folders are created as needed).
// ============================================================

use super::transfer::events::ItemError;
use super::transfer::OpResult;
use crate::errors::{AppError, AppResult};
use crate::vfs::caps::{self, Capability};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::ipc::Channel;
use ts_rs::TS;

/// A comparison result for a file (relative to the two roots).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DiffEntry {
    /// Relative path (separated by "/"), identical on both sides.
    pub rel: String,
    /// Base name of the file.
    pub name: String,
    /// Exists left / right.
    pub left: bool,
    pub right: bool,
    #[ts(type = "number")]
    pub left_size: u64,
    #[ts(type = "number")]
    pub right_size: u64,
    #[ts(type = "number")]
    pub left_modified: u64,
    #[ts(type = "number")]
    pub right_modified: u64,
    /// "same" | "left_only" | "right_only" | "newer_left" | "newer_right" | "different"
    pub status: String,
}

/// Upper limit of comparison entries. Prevents very large trees from
/// overloading memory / the frontend; beyond it the result is truncated and
/// the frontend shows a warning.
const MAX_ENTRIES: usize = 200_000;

type Meta = (u64, u64); // (size, mtime in Unix seconds)

/// Reads one directory level: files (name → meta) and subfolder names.
/// Symlinks are skipped; sorted collections for a stable order.
fn read_level(
    dir: &Path,
    missing_ok: bool,
) -> AppResult<(BTreeMap<String, Meta>, BTreeSet<String>)> {
    let mut files = BTreeMap::new();
    let mut subdirs = BTreeSet::new();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if missing_ok && e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((files, subdirs))
        }
        Err(e) => return Err(e.into()),
    };
    for entry in rd {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if meta.is_dir() {
            subdirs.insert(name);
        } else {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            files.insert(name, (meta.len(), mtime));
        }
    }
    Ok((files, subdirs))
}

fn status_of(l: Option<&Meta>, r: Option<&Meta>) -> &'static str {
    match (l, r) {
        (Some(_), None) => "left_only",
        (None, Some(_)) => "right_only",
        (Some((ls, lm)), Some((rs, rm))) => {
            if *lm == 0 || *rm == 0 {
                "different"
            } else if ls == rs && lm == rm {
                "same"
            } else if lm > rm {
                "newer_left"
            } else if rm > lm {
                "newer_right"
            } else {
                "different"
            }
        }
        (None, None) => "different",
    }
}

/// Compares level by level and sends each directory batch immediately
/// over the channel to the frontend (streaming), before descending into
/// subfolders. `count` counts the already-sent entries; on
/// reaching `MAX_ENTRIES` it aborts. Returns `true` = limit
/// reached (result truncated).
pub(crate) fn walk_compare(
    lroot: &Path,
    rroot: &Path,
    rel: &str,
    recursive: bool,
    channel: &dyn Fn(Vec<DiffEntry>),
    count: &mut usize,
) -> AppResult<bool> {
    let ljoin = if rel.is_empty() {
        lroot.to_path_buf()
    } else {
        lroot.join(rel)
    };
    let rjoin = if rel.is_empty() {
        rroot.to_path_buf()
    } else {
        rroot.join(rel)
    };
    let (lfiles, lsubs) = read_level(&ljoin, !rel.is_empty())?;
    let (rfiles, rsubs) = read_level(&rjoin, !rel.is_empty())?;

    // Compare the files of this level (names of both sides, sorted).
    let mut names: BTreeSet<&String> = BTreeSet::new();
    names.extend(lfiles.keys());
    names.extend(rfiles.keys());

    let mut batch = Vec::new();
    let mut limit_hit = false;
    for name in names {
        if *count >= MAX_ENTRIES {
            limit_hit = true;
            break;
        }
        let l = lfiles.get(name);
        let r = rfiles.get(name);
        let (left_size, left_modified) = l.copied().unwrap_or((0, 0));
        let (right_size, right_modified) = r.copied().unwrap_or((0, 0));
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", rel, name)
        };
        batch.push(DiffEntry {
            rel: child_rel,
            name: name.clone(),
            left: l.is_some(),
            right: r.is_some(),
            left_size,
            right_size,
            left_modified,
            right_modified,
            status: status_of(l, r).to_string(),
        });
        *count += 1;
    }
    if !batch.is_empty() {
        channel(batch);
    }
    if limit_hit || *count >= MAX_ENTRIES {
        return Ok(true);
    }

    if recursive {
        let mut subs: BTreeSet<&String> = BTreeSet::new();
        subs.extend(lsubs.iter());
        subs.extend(rsubs.iter());
        for sub in subs {
            let child_rel = if rel.is_empty() {
                sub.clone()
            } else {
                format!("{}/{}", rel, sub)
            };
            if walk_compare(lroot, rroot, &child_rel, recursive, channel, count)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Compares two directory trees and streams the results per
/// directory over `on_batch`. Runs on a background thread
/// (`spawn_blocking`) so large trees don't block the UI.
/// Returns `true` = the result was truncated at `MAX_ENTRIES`.
#[tauri::command]
pub async fn compare_dirs(
    left: String,
    right: String,
    recursive: bool,
    on_batch: Channel<Vec<DiffEntry>>,
) -> AppResult<bool> {
    // Both trees are walked with `std::fs`; each side is asked
    // separately, so a local left pane never licenses a remote right
    // one (TICKET-031).
    caps::require_pair(
        &left,
        Capability::CompareDirs,
        &right,
        Capability::CompareDirs,
    )?;
    let lp = PathBuf::from(&left);
    let rp = PathBuf::from(&right);
    if !lp.is_dir() {
        return Err(AppError::from(format!("{}: kein Verzeichnis", left)));
    }
    if !rp.is_dir() {
        return Err(AppError::from(format!("{}: kein Verzeichnis", right)));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0usize;
        walk_compare(
            &lp,
            &rp,
            "",
            recursive,
            &|batch| {
                let _ = on_batch.send(batch);
            },
            &mut count,
        )
    })
    .await
    .map_err(AppError::from)?
}

/// Copies files from source → target (blocking).
fn sync_copy_impl(items: Vec<(String, String)>) -> OpResult {
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for (src, dst) in &items {
        // `fs::copy` — one pair at a time, both ends checked, so a
        // single unsupported pair fails on its own instead of aborting
        // the whole synchronization (TICKET-031).
        if let Err(e) =
            caps::require_pair(src, Capability::CompareDirs, dst, Capability::CompareDirs)
        {
            errors.push(ItemError::new(src, e.with("name", dst)));
            continue;
        }
        // Refuse aliases too: fs::copy on the same file may truncate the source.
        let same = fs::canonicalize(src)
            .ok()
            .zip(fs::canonicalize(dst).ok())
            .is_some_and(|(a, b)| a == b);
        #[cfg(unix)]
        let same = same || {
            use std::os::unix::fs::MetadataExt;
            fs::metadata(src)
                .ok()
                .zip(fs::metadata(dst).ok())
                .is_some_and(|(a, b)| a.dev() == b.dev() && a.ino() == b.ino())
        };
        if same {
            errors.push(ItemError::new(
                src,
                AppError::from("Source and destination are the same file"),
            ));
            continue;
        }
        let dpath = Path::new(dst);
        if let Some(parent) = dpath.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                errors.push(ItemError::new(dst, e.into()));
                continue;
            }
        }
        match fs::copy(src, dst) {
            Ok(_) => ok += 1,
            Err(e) => errors.push(ItemError::new(src, AppError::from(e).with("name", dst))),
        }
    }
    OpResult { ok, errors }
}

/// Copies files from source → target (pairs of absolute paths). Missing
/// target folders are created; existing target files are overwritten.
/// Runs on a background thread so as not to block the UI.
#[tauri::command]
pub async fn sync_copy(items: Vec<(String, String)>) -> OpResult {
    match tauri::async_runtime::spawn_blocking(move || sync_copy_impl(items)).await {
        Ok(res) => res,
        Err(e) => OpResult {
            ok: 0,
            errors: vec![ItemError::new("", AppError::from(e.to_string()))],
        },
    }
}

// ============================================================
// Single-level comparison for the "Mark" menu (TICKET-059).
//
// The pane comparison works on the two listings the panes already
// hold, which is what makes it provider-neutral: the very same
// classification serves local folders, every remote provider and the
// inside of an archive, without a second directory scan and without
// `std::fs` — a remote pane has no local path to walk in the first
// place. Only the classification lives here, so the rules below are
// unit-testable and cannot drift from `status_of` above.
// ============================================================

/// One entry of a directory level as the pane already knows it.
#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LevelEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    #[ts(type = "number")]
    pub size: u64,
    /// Unix seconds; 0 means the provider reported no timestamp.
    #[ts(type = "number")]
    pub modified: u64,
}

/// Classification of one name across both levels.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LevelDiff {
    pub name: String,
    pub left: bool,
    pub right: bool,
    /// "same" | "left_only" | "right_only" | "newer_left" |
    /// "newer_right" | "different" | "unknown"
    pub status: String,
}

/// True when an entry cannot take part in a metadata comparison:
/// a symlink (whose metadata describes the link, not the target) or
/// a missing timestamp. Such entries are never called "same".
fn uncomparable(e: &LevelEntry) -> bool {
    e.is_symlink || e.modified == 0
}

/// Classifies one name. Equality here is *metadata* equality (name,
/// size and timestamp) — it is explicitly not a content comparison,
/// which is what the file comparison under "Commands" is for.
fn classify_pair(l: Option<&LevelEntry>, r: Option<&LevelEntry>) -> &'static str {
    match (l, r) {
        (Some(_), None) => "left_only",
        (None, Some(_)) => "right_only",
        (None, None) => "unknown",
        (Some(a), Some(b)) => {
            // A name that is a folder on one side and a file on the
            // other is a type conflict, not a difference in content.
            if a.is_dir != b.is_dir || uncomparable(a) || uncomparable(b) {
                return "unknown";
            }
            if a.size == b.size && a.modified == b.modified {
                "same"
            } else if a.modified > b.modified {
                "newer_left"
            } else if b.modified > a.modified {
                "newer_right"
            } else {
                // Same timestamp, different size: a real difference,
                // but neither side is the newer one.
                "different"
            }
        }
    }
}

/// Compares two directory levels by name. Folders are classified as
/// well (so a type conflict is visible) but carry no "newer" verdict,
/// because a folder timestamp says nothing about its contents.
pub fn compare_levels_impl(left: Vec<LevelEntry>, right: Vec<LevelEntry>) -> Vec<LevelDiff> {
    let index = |v: &Vec<LevelEntry>| -> BTreeMap<String, LevelEntry> {
        v.iter().map(|e| (e.name.clone(), e.clone())).collect()
    };
    let l = index(&left);
    let r = index(&right);
    let mut names: BTreeSet<&String> = BTreeSet::new();
    names.extend(l.keys());
    names.extend(r.keys());

    names
        .into_iter()
        .map(|name| {
            let le = l.get(name);
            let re = r.get(name);
            let mut status = classify_pair(le, re);
            if status == "newer_left" || status == "newer_right" {
                if let (Some(a), Some(b)) = (le, re) {
                    if a.is_dir && b.is_dir {
                        status = "different";
                    }
                }
            }
            LevelDiff {
                name: name.clone(),
                left: le.is_some(),
                right: re.is_some(),
                status: status.to_string(),
            }
        })
        .collect()
}

/// Compares the two visible directory levels the panes hand over.
/// Async so a very large pair of listings cannot block the UI thread
/// and so the caller can drop a result that arrived too late.
#[tauri::command]
pub async fn compare_levels(left: Vec<LevelEntry>, right: Vec<LevelEntry>) -> Vec<LevelDiff> {
    tauri::async_runtime::spawn_blocking(move || compare_levels_impl(left, right))
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod level_tests {
    use super::*;

    fn f(name: &str, size: u64, modified: u64) -> LevelEntry {
        LevelEntry {
            name: name.into(),
            is_dir: false,
            is_symlink: false,
            size,
            modified,
        }
    }

    fn status(diffs: &[LevelDiff], name: &str) -> String {
        diffs
            .iter()
            .find(|d| d.name == name)
            .map(|d| d.status.clone())
            .unwrap_or_else(|| panic!("{} fehlt im Ergebnis", name))
    }

    #[test]
    fn only_one_side_missing_and_identical() {
        let diffs = compare_levels_impl(
            vec![f("nur_links.txt", 1, 100), f("beide.txt", 10, 500)],
            vec![f("nur_rechts.txt", 1, 100), f("beide.txt", 10, 500)],
        );
        assert_eq!(status(&diffs, "nur_links.txt"), "left_only");
        assert_eq!(status(&diffs, "nur_rechts.txt"), "right_only");
        assert_eq!(status(&diffs, "beide.txt"), "same");
    }

    #[test]
    fn newer_side_wins_over_size() {
        let diffs = compare_levels_impl(vec![f("a", 10, 900)], vec![f("a", 99, 100)]);
        assert_eq!(status(&diffs, "a"), "newer_left");
        let diffs = compare_levels_impl(vec![f("a", 10, 100)], vec![f("a", 99, 900)]);
        assert_eq!(status(&diffs, "a"), "newer_right");
    }

    #[test]
    fn same_time_other_size_is_a_difference_not_newer() {
        let diffs = compare_levels_impl(vec![f("a", 10, 500)], vec![f("a", 20, 500)]);
        assert_eq!(status(&diffs, "a"), "different");
    }

    #[test]
    fn missing_timestamps_are_never_same() {
        // Identical size, no timestamp on either side: not safely
        // comparable, so it must not be hidden as "same".
        let diffs = compare_levels_impl(vec![f("a", 10, 0)], vec![f("a", 10, 0)]);
        assert_eq!(status(&diffs, "a"), "unknown");
        let diffs = compare_levels_impl(vec![f("a", 10, 500)], vec![f("a", 10, 0)]);
        assert_eq!(status(&diffs, "a"), "unknown");
    }

    #[test]
    fn symlinks_are_not_comparable() {
        let mut link = f("a", 10, 500);
        link.is_symlink = true;
        let diffs = compare_levels_impl(vec![link], vec![f("a", 10, 500)]);
        assert_eq!(status(&diffs, "a"), "unknown");
    }

    #[test]
    fn type_conflict_is_unknown() {
        let mut dir = f("a", 0, 500);
        dir.is_dir = true;
        let diffs = compare_levels_impl(vec![dir], vec![f("a", 0, 500)]);
        assert_eq!(status(&diffs, "a"), "unknown");
    }

    #[test]
    fn folders_are_compared_but_never_called_newer() {
        let mut l = f("ordner", 0, 900);
        l.is_dir = true;
        let mut r = f("ordner", 0, 100);
        r.is_dir = true;
        let diffs = compare_levels_impl(vec![l.clone()], vec![r]);
        assert_eq!(status(&diffs, "ordner"), "different");
        // Identical folder metadata still counts as equal.
        let diffs = compare_levels_impl(vec![l.clone()], vec![l]);
        assert_eq!(status(&diffs, "ordner"), "same");
    }

    #[test]
    fn empty_levels_produce_no_result() {
        assert!(compare_levels_impl(vec![], vec![]).is_empty());
        let diffs = compare_levels_impl(vec![f("a", 1, 1)], vec![]);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].left && !diffs[0].right);
    }

    #[test]
    fn names_differing_only_in_case_stay_separate() {
        let diffs = compare_levels_impl(vec![f("Datei", 1, 1)], vec![f("datei", 1, 1)]);
        assert_eq!(status(&diffs, "Datei"), "left_only");
        assert_eq!(status(&diffs, "datei"), "right_only");
    }
}

#[cfg(test)]
mod paired_safety_tests {
    use super::*;
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn temp() -> Temp {
        let path = std::env::temp_dir().join(format!(
            "hortcmd-compare-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Temp(path)
    }
    #[test]
    fn missing_metadata_and_read_errors_are_not_equal() {
        assert_eq!(status_of(Some(&(10, 0)), Some(&(10, 0))), "different");
        let t = temp();
        let file = t.0.join("file");
        fs::write(&file, b"data").unwrap();
        assert!(read_level(&file, false).is_err());
        assert!(read_level(&t.0.join("missing"), false).is_err());
        assert!(read_level(&t.0.join("missing"), true).is_ok());
    }
    #[test]
    fn self_copy_preserves_source_contents() {
        let t = temp();
        let file = t.0.join("file");
        fs::write(&file, b"keep me").unwrap();
        let source = file.to_string_lossy().into_owned();
        let result = sync_copy_impl(vec![(source.clone(), source.clone())]);
        assert_eq!(result.ok, 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(fs::read(&file).unwrap(), b"keep me");
        #[cfg(unix)]
        {
            let alias = t.0.join("alias");
            fs::hard_link(&file, &alias).unwrap();
            let result = sync_copy_impl(vec![(source, alias.to_string_lossy().into_owned())]);
            assert_eq!(result.ok, 0);
            assert_eq!(fs::read(&file).unwrap(), b"keep me");
        }
    }
}
