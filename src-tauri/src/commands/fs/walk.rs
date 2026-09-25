// ============================================================
// Reusable local directory traversal (Ticket 013).
//
// Built on `jwalk`: the directory reads run on a bounded worker pool,
// the entries are consumed sequentially by the caller. That keeps the
// hit limits, batching and cancellation logic lock-free while the
// expensive part (`read_dir` + `stat`) is parallel.
//
// Guarantees:
//   * Ignored folder names are pruned *before* descending – their
//     subtrees are never read.
//   * Symbolic links are never followed and never reported. On Windows
//     this also covers junctions/mount points, because `FileType::
//     is_symlink` is true for the reparse tags that can form a loop
//     (`IO_REPARSE_TAG_SYMLINK`, `IO_REPARSE_TAG_MOUNT_POINT`). Other
//     reparse points (e.g. cloud placeholders) stay normal entries.
//   * No recursion: `jwalk` keeps its own explicit queue, so deep trees
//     cannot overflow the call stack.
//   * Unreadable, vanished or changing entries are counted, never fatal.
//   * Cancellation is checked before every directory read and on every
//     entry, so a running walk stops within one directory.
//
// The component is deliberately local-only: remote trees are enumerated
// through the VFS provider interface and never reach this walker.
// ============================================================

use jwalk::{Parallelism, WalkDirGeneric};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

/// Default reader threads. Measured sweet spot: two threads halve the
/// scan time of a wide tree, four add no speed but a rayon pool worth
/// several MB (see `docs/traversal-benchmark.md`).
pub const WALK_THREADS: usize = 2;

/// Hard upper bound for the reader threads. Traversal is I/O bound;
/// more threads mostly add open handles and contention.
pub const MAX_WALK_THREADS: usize = 4;

/// Per-entry state filled in while the parent directory is processed.
#[derive(Clone, Debug, Default)]
pub struct EntryState {
    /// Directories only: the folder has no children at all (computed
    /// before ignore-pruning, exactly like a plain `read_dir`).
    pub empty: bool,
}

/// One entry produced by the walk (never the root itself).
pub struct WalkEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Modification time as a unix timestamp (0 if unavailable).
    pub modified: u64,
    /// Directories only: the folder is empty.
    pub dir_empty: bool,
}

/// What the consumer wants the walk to do next.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Flow {
    Continue,
    /// Enough data – stop reading further directories.
    Stop,
}

/// Which entries the caller is interested in. Everything that is not
/// wanted is dropped while the parent directory is processed, so it is
/// neither `stat`ed nor handed to the consumer.
pub struct WalkOptions {
    /// Folder names skipped at every level (not descended, not reported).
    pub ignore_dirs: Vec<String>,
    pub want_files: bool,
    pub want_dirs: bool,
    /// Fill `dir_empty` (costs one extra `read_dir` per folder, done on
    /// the worker threads).
    pub detect_empty_dirs: bool,
    /// Reader threads, clamped to `1..=MAX_WALK_THREADS`.
    pub threads: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            ignore_dirs: Vec::new(),
            want_files: true,
            want_dirs: false,
            detect_empty_dirs: false,
            threads: WALK_THREADS,
        }
    }
}

/// How a walk ended, plus what it saw.
#[derive(Default)]
pub struct WalkReport {
    pub files: u64,
    pub dirs: u64,
    /// Entries or directories that could not be read and were skipped.
    pub unreadable: u64,
    /// The cancel flag was set while walking.
    pub cancelled: bool,
    /// The consumer asked to stop (limit reached).
    pub stopped: bool,
}

#[derive(Default)]
struct Counters {
    files: AtomicU64,
    dirs: AtomicU64,
    unreadable: AtomicU64,
}

pub(crate) fn mtime_of(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parallelism(threads: usize) -> Parallelism {
    match threads.clamp(1, MAX_WALK_THREADS) {
        1 => Parallelism::Serial,
        n => Parallelism::RayonNewPool(n),
    }
}

/// Walks `root` and hands every wanted entry to `on_entry`.
///
/// The callback runs on the calling thread in traversal order, so it may
/// keep mutable state without synchronisation. Returning [`Flow::Stop`]
/// ends the walk; queued directory reads are dropped.
pub fn walk_local(
    root: &Path,
    opts: &WalkOptions,
    cancel: &Arc<AtomicBool>,
    mut on_entry: impl FnMut(WalkEntry) -> Flow,
) -> WalkReport {
    let stop = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::default());

    let ignore: Arc<Vec<String>> = Arc::new(opts.ignore_dirs.clone());
    let want_files = opts.want_files;
    let want_dirs = opts.want_dirs;
    let detect_empty = opts.detect_empty_dirs && want_dirs;
    let cancel_read = Arc::clone(cancel);
    let stop_read = Arc::clone(&stop);
    let counters_read = Arc::clone(&counters);

    let walker = WalkDirGeneric::<((), EntryState)>::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .sort(false)
        .min_depth(1)
        .parallelism(parallelism(opts.threads))
        .process_read_dir(move |depth, _path, _state, children| {
            // `jwalk` also calls this once for the root entry itself
            // (depth `None`). The root is neither a hit nor prunable –
            // a folder called "target" must still be searchable when it
            // is the folder the user started in.
            if depth.is_none() {
                return;
            }
            // Cancelling drops the whole level, so no further directory
            // below it is ever queued.
            if cancel_read.load(Ordering::Relaxed) || stop_read.load(Ordering::Relaxed) {
                children.clear();
                return;
            }
            let mut files = 0u64;
            let mut dirs = 0u64;
            let mut bad = 0u64;
            children.retain(|child| match child {
                Ok(entry) => {
                    let ft = entry.file_type();
                    // Links are neither followed nor reported – that is
                    // what makes a → b → a loops impossible.
                    if ft.is_symlink() {
                        return false;
                    }
                    if ft.is_dir() {
                        let name = entry.file_name().to_string_lossy();
                        if ignore.iter().any(|ig| ig.as_str() == name) {
                            return false;
                        }
                        dirs += 1;
                        // Directories always stay in the list – they have
                        // to be descended even when only files are wanted;
                        // unwanted ones are dropped in the consumer.
                        true
                    } else {
                        files += 1;
                        want_files
                    }
                }
                Err(_) => {
                    bad += 1;
                    false
                }
            });
            if detect_empty {
                for child in children.iter_mut().flatten() {
                    if child.file_type().is_dir() {
                        child.client_state.empty = is_empty_dir(&child.path());
                    }
                }
            }
            counters_read.files.fetch_add(files, Ordering::Relaxed);
            counters_read.dirs.fetch_add(dirs, Ordering::Relaxed);
            counters_read.unreadable.fetch_add(bad, Ordering::Relaxed);
        });

    let mut report = WalkReport::default();
    for item in walker {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let entry = match item {
            Ok(entry) => entry,
            // An entry or a whole directory that vanished or is not
            // readable (`fs::read_dir` failed): counted and skipped, the
            // walk continues with the siblings.
            Err(_) => {
                counters.unreadable.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let is_dir = entry.file_type().is_dir();
        if is_dir && !want_dirs {
            continue;
        }
        let path = entry.path();
        // Files can disappear between listing and `stat`; such an entry
        // is skipped instead of aborting the search.
        let (size, modified) = if is_dir {
            (0, fs::metadata(&path).map(|m| mtime_of(&m)).unwrap_or(0))
        } else {
            match entry.metadata() {
                Ok(meta) => (meta.len(), mtime_of(&meta)),
                Err(_) => {
                    counters.unreadable.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
        };
        let flow = on_entry(WalkEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path,
            is_dir,
            size,
            modified,
            dir_empty: entry.client_state.empty,
        });
        if flow == Flow::Stop {
            stop.store(true, Ordering::Relaxed);
            report.stopped = true;
            break;
        }
    }

    if cancel.load(Ordering::Relaxed) {
        report.cancelled = true;
    }
    report.files = counters.files.load(Ordering::Relaxed);
    report.dirs = counters.dirs.load(Ordering::Relaxed);
    report.unreadable = counters.unreadable.load(Ordering::Relaxed);
    report
}

/// True if the folder has no children. An unreadable folder is not
/// "empty" – we simply do not know.
fn is_empty_dir(path: &Path) -> bool {
    match fs::read_dir(path) {
        Ok(mut rd) => rd.next().is_none(),
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "walk_bench.rs"]
mod bench;

/// Convenience for callers that only need the entries (tests, tools).
#[allow(dead_code)]
pub fn collect_local(root: &Path, opts: &WalkOptions) -> (Vec<WalkEntry>, WalkReport) {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut out = Vec::new();
    let report = walk_local(root, opts, &cancel, |entry| {
        out.push(entry);
        Flow::Continue
    });
    (out, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rc-walk-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn opts(ignore: &[&str]) -> WalkOptions {
        WalkOptions {
            ignore_dirs: ignore.iter().map(|s| s.to_string()).collect(),
            want_files: true,
            want_dirs: true,
            ..WalkOptions::default()
        }
    }

    fn names(entries: &[WalkEntry]) -> Vec<String> {
        let mut out: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        out.sort();
        out
    }

    #[test]
    fn reports_files_and_dirs_but_not_the_root() {
        let d = tmp("basic");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.txt"), b"hallo").unwrap();
        fs::write(d.join("sub/b.txt"), b"x").unwrap();

        let (entries, report) = collect_local(&d, &opts(&[]));
        assert_eq!(names(&entries), vec!["a.txt", "b.txt", "sub"]);
        assert_eq!(report.files, 2);
        assert_eq!(report.dirs, 1);
        assert_eq!(report.unreadable, 0);
        assert!(!report.cancelled && !report.stopped);
        let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(a.size, 5);
        assert!(a.modified > 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn ignored_dirs_are_pruned_before_descending() {
        let d = tmp("prune");
        fs::create_dir_all(d.join("node_modules/inner")).unwrap();
        fs::write(d.join("node_modules/x.js"), b"x").unwrap();
        fs::write(d.join("keep.js"), b"y").unwrap();

        let (entries, report) = collect_local(&d, &opts(&["node_modules"]));
        assert_eq!(names(&entries), vec!["keep.js"]);
        // The subtree was never read, so nothing in it was even counted.
        assert_eq!(report.files, 1);
        assert_eq!(report.dirs, 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_root_itself_is_never_pruned() {
        let d = tmp("root-named-target");
        let root = d.join("target");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"x").unwrap();

        let (entries, _) = collect_local(&root, &opts(&["target"]));
        assert_eq!(names(&entries), vec!["a.txt"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_dirs_are_detected() {
        let d = tmp("empty");
        fs::create_dir_all(d.join("leer")).unwrap();
        fs::create_dir_all(d.join("voll")).unwrap();
        fs::write(d.join("voll/x.txt"), b"x").unwrap();

        let o = WalkOptions {
            want_files: false,
            want_dirs: true,
            detect_empty_dirs: true,
            ..WalkOptions::default()
        };
        let (entries, report) = collect_local(&d, &o);
        let empty: Vec<_> = entries
            .iter()
            .filter(|e| e.dir_empty)
            .map(|e| &e.name)
            .collect();
        assert_eq!(empty, vec!["leer"]);
        // Files are still counted even though they are not reported.
        assert_eq!(report.files, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_folder_holding_only_ignored_names_is_not_empty() {
        let d = tmp("only-ignored");
        fs::create_dir_all(d.join("outer/node_modules")).unwrap();

        let o = WalkOptions {
            ignore_dirs: vec!["node_modules".into()],
            want_files: false,
            want_dirs: true,
            detect_empty_dirs: true,
            ..WalkOptions::default()
        };
        let (entries, _) = collect_local(&d, &o);
        let outer = entries.iter().find(|e| e.name == "outer").unwrap();
        assert!(!outer.dir_empty, "outer has a child, even if it is ignored");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn deep_tree_does_not_grow_the_call_stack() {
        let d = tmp("deep");
        let mut cur = d.clone();
        // One-character names – the depth is capped by PATH_MAX.
        for _ in 0..400 {
            cur = cur.join("d");
            fs::create_dir_all(&cur).unwrap();
        }
        let (entries, report) = collect_local(&d, &opts(&[]));
        assert_eq!(entries.len(), 400);
        assert_eq!(report.dirs, 400);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stop_ends_the_walk() {
        let d = tmp("stop");
        for i in 0..50 {
            fs::write(d.join(format!("f{}.txt", i)), b"x").unwrap();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let mut seen = 0;
        let report = walk_local(&d, &opts(&[]), &cancel, |_| {
            seen += 1;
            if seen >= 3 {
                Flow::Stop
            } else {
                Flow::Continue
            }
        });
        assert_eq!(seen, 3);
        assert!(report.stopped);
        assert!(!report.cancelled);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cancel_stops_the_walk() {
        let d = tmp("cancel");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/a.txt"), b"x").unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let mut seen = 0;
        let report = walk_local(&d, &opts(&[]), &cancel, |_| {
            seen += 1;
            Flow::Continue
        });
        assert_eq!(seen, 0);
        assert!(report.cancelled);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cancel_during_the_walk_is_reported() {
        let d = tmp("cancel-mid");
        for i in 0..50 {
            fs::write(d.join(format!("f{}.txt", i)), b"x").unwrap();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let mut seen = 0;
        let report = walk_local(&d, &opts(&[]), &cancel, move |_| {
            seen += 1;
            if seen == 2 {
                flag.store(true, Ordering::Relaxed);
            }
            Flow::Continue
        });
        assert!(report.cancelled);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn files_that_vanish_while_walking_are_counted_not_fatal() {
        let d = tmp("vanish");
        for i in 0..20 {
            fs::write(d.join(format!("f{}.txt", i)), b"x").unwrap();
        }
        // Serial so the whole folder is listed before the first `stat`.
        let o = WalkOptions {
            threads: 1,
            want_dirs: true,
            ..WalkOptions::default()
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let dir = d.clone();
        let mut seen = 0;
        let report = walk_local(&d, &o, &cancel, move |_| {
            if seen == 0 {
                // Everything the walker has listed but not yet read.
                for i in 0..20 {
                    let _ = fs::remove_file(dir.join(format!("f{}.txt", i)));
                }
            }
            seen += 1;
            Flow::Continue
        });
        assert_eq!(report.files, 20);
        assert!(report.unreadable > 0, "vanished files must be counted");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_neither_followed_nor_reported() {
        let d = tmp("links");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/a.txt"), b"x").unwrap();
        // sub/back → d: following it would recurse forever.
        std::os::unix::fs::symlink(&d, d.join("sub/back")).unwrap();
        std::os::unix::fs::symlink(d.join("sub/a.txt"), d.join("link.txt")).unwrap();

        let (entries, report) = collect_local(&d, &opts(&[]));
        assert_eq!(names(&entries), vec!["a.txt", "sub"]);
        assert_eq!(report.files, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folder_is_counted_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("unreadable");
        fs::write(d.join("a.txt"), b"x").unwrap();
        let locked = d.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("hidden.txt"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let (entries, report) = collect_local(&d, &opts(&[]));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root ignores the permission bits – then nothing is
        // skipped and the hidden file shows up instead.
        if report.unreadable > 0 {
            assert_eq!(names(&entries), vec!["a.txt", "locked"]);
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// Windows equivalent of the symlink-loop test: a directory junction
    /// (`IO_REPARSE_TAG_MOUNT_POINT`) is a reparse point that
    /// `FileType::is_symlink` reports as a link, so it is pruned exactly
    /// like a Unix symlink and cannot produce a loop.
    #[cfg(windows)]
    #[test]
    fn directory_junctions_are_not_descended() {
        use std::process::Command;
        let d = tmp("junction");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/a.txt"), b"x").unwrap();
        let link = d.join("sub").join("back");
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&d)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            return; // mklink /J unavailable – nothing to assert
        }
        let (entries, _) = collect_local(&d, &opts(&[]));
        assert_eq!(names(&entries), vec!["a.txt", "sub"]);
        let _ = fs::remove_dir_all(&d);
    }
}
