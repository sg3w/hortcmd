// ============================================================
// Search over a directory tree. Four modes:
//   files        – name and/or content search (glob or regex)
//   duplicates   – find identical files (size, then SHA-256)
//   empty_dirs   – empty folders
//   large_files  – files from a minimum size (sorted by size)
//
// Runs asynchronously via `spawn_blocking` and streams matches in
// batches over a Tauri `Channel`. The tree is read by the reusable
// local walker (`super::walk`, jwalk-backed): ignored folders are
// pruned before descending, links are never followed.
//
// Limits (all enforced, all reported through `SearchSummary::status`):
//   * `Limits::max_hits` matches – further matches are not produced,
//     the walk stops right there ("truncated").
//   * `Limits::dup_candidates` files remembered in duplicate mode – the
//     only mode that must see the whole tree before it can answer
//     ("limited").
//   * `Limits::large_candidates` files kept in a bounded heap in
//     large-file mode, so sorting never buffers the whole tree.
//   * `MAX_CONTENT_BYTES` per file for the content search.
// Unreadable entries are counted and reported ("partial"); a cancel
// token or a closed result channel ends the run ("cancelled").
// ============================================================

use super::cancel;
use super::walk::{walk_local, Flow, WalkOptions, WALK_THREADS};
use crate::errors::{AppError, AppResult};
use crate::vfs::caps::{self, Capability};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::ipc::Channel;
use ts_rs::TS;

/// A search hit (file or folder).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub modified: u64,
    /// Additional info: match line (content), size, or group hint.
    pub detail: String,
    /// Duplicate group (>0 in "duplicates" mode; otherwise 0).
    pub group: u32,
}

/// How a search ended. Everything except `Complete` means the result
/// list is not the full answer.
#[derive(Serialize, TS, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum SearchStatus {
    /// Whole tree read, every match reported.
    Complete,
    /// The match limit was reached; the scan stopped early.
    Truncated,
    /// A work/memory limit of an aggregating mode was reached.
    Limited,
    /// The tree was read completely, but entries had to be skipped.
    Partial,
    /// Cancelled by the frontend, or the result channel was closed.
    Cancelled,
}

/// Result of a search run.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SearchSummary {
    pub status: SearchStatus,
    /// Matches sent over the channel.
    #[ts(type = "number")]
    pub hits: u64,
    /// Files seen while walking.
    #[ts(type = "number")]
    pub files: u64,
    /// Folders seen while walking.
    #[ts(type = "number")]
    pub dirs: u64,
    /// Entries skipped because they could not be read.
    #[ts(type = "number")]
    pub unreadable: u64,
}

/// Search options from the frontend.
#[derive(Deserialize)]
pub struct SearchOptions {
    /// "files" | "duplicates" | "empty_dirs" | "large_files"
    pub mode: String,
    /// Name pattern (glob or regex); empty = all names.
    pub name: String,
    pub name_regex: bool,
    /// Content search (only "files" mode); empty = no content search.
    pub content: String,
    pub content_regex: bool,
    pub case_sensitive: bool,
    /// Folder names that are skipped (at every level).
    pub ignore_dirs: Vec<String>,
    /// Minimum size in bytes (only "large_files" mode).
    #[serde(default)]
    pub min_size: u64,
}

const MAX_CONTENT_BYTES: u64 = 8 * 1024 * 1024; // Inhaltssuche nur bis 8 MB/Datei
/// Cancel token namespace (see `super::cancel`).
const KIND: &str = "search";

/// The memory/work bounds of a search run. Kept in one place so they
/// are documented together – and so the tests can shrink them instead
/// of building fixtures with 50 000 files.
pub(crate) struct Limits {
    /// Matches after which the scan stops ("truncated").
    max_hits: usize,
    /// Matches per channel message – small enough that the first
    /// results show up immediately, large enough to keep the IPC cheap.
    batch: usize,
    /// Files duplicate mode may remember before it gives up ("limited").
    /// It is the only mode that must see the whole tree to answer.
    dup_candidates: usize,
    /// Files large-file mode keeps in its bounded heap, so sorting never
    /// buffers the whole tree.
    large_candidates: usize,
}

impl Limits {
    pub(crate) const DEFAULT: Limits = Limits {
        max_hits: 50_000,
        batch: 200,
        dup_candidates: 300_000,
        large_candidates: 50_000,
    };
}

/// Converts a glob pattern (`*`, `?`, multiple separated by space/`;`) to regex.
fn glob_to_regex(pattern: &str, case_sensitive: bool) -> AppResult<regex::Regex> {
    build_regex(&super::glob::pattern(pattern), case_sensitive)
}

fn build_regex(pattern: &str, case_sensitive: bool) -> AppResult<regex::Regex> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| AppError::from(format!("Ungültiger Ausdruck: {}", e)))
}

/// A name matcher (regex or glob), or "everything" for an empty pattern.
enum NameMatch {
    Any,
    Re(regex::Regex),
}

impl NameMatch {
    fn build(opts: &SearchOptions) -> AppResult<Self> {
        if opts.name.trim().is_empty() {
            return Ok(NameMatch::Any);
        }
        let re = if opts.name_regex {
            build_regex(&opts.name, opts.case_sensitive)?
        } else {
            glob_to_regex(&opts.name, opts.case_sensitive)?
        };
        Ok(NameMatch::Re(re))
    }
    fn matches(&self, name: &str) -> bool {
        match self {
            NameMatch::Any => true,
            NameMatch::Re(re) => re.is_match(name),
        }
    }
}

fn hit(path: &Path, size: u64, modified: u64, detail: String, group: u32) -> SearchHit {
    SearchHit {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        is_dir: false,
        size,
        modified,
        detail,
        group,
    }
}

/// Where a batch of matches goes.
///
/// Behind a trait for the same reason progress reports are (TICKET-035):
/// a `Channel` only exists inside a running application, and the search
/// is worth running from the MCP interface too (TICKET-041). Production
/// behaviour is unchanged — the implementation below sends exactly the
/// event it always did.
pub(crate) trait HitSink {
    /// `false` when the receiver is gone, which stops the walk.
    fn send(&self, batch: Vec<SearchHit>) -> bool;
}

impl HitSink for Channel<Vec<SearchHit>> {
    fn send(&self, batch: Vec<SearchHit>) -> bool {
        Channel::send(self, batch).is_ok()
    }
}

/// Collects matches into bounded batches and pushes them to the sink.
/// Enforces the hit limit and notices a closed receiver, so the walk can
/// stop instead of scanning on into the void.
struct Sink<'a> {
    channel: &'a dyn HitSink,
    limits: &'a Limits,
    batch: Vec<SearchHit>,
    sent: usize,
    truncated: bool,
    disconnected: bool,
}

impl<'a> Sink<'a> {
    fn new(channel: &'a dyn HitSink, limits: &'a Limits) -> Self {
        Self {
            channel,
            limits,
            batch: Vec::new(),
            sent: 0,
            truncated: false,
            disconnected: false,
        }
    }

    /// Accepts a match. `false` = stop searching (limit or closed channel).
    fn push(&mut self, hit: SearchHit) -> bool {
        if self.disconnected {
            return false;
        }
        self.batch.push(hit);
        if self.total() >= self.limits.max_hits {
            self.truncated = true;
            self.flush();
            return false;
        }
        if self.batch.len() >= self.limits.batch {
            self.flush();
        }
        !self.disconnected
    }

    fn total(&self) -> usize {
        self.sent + self.batch.len()
    }

    fn flush(&mut self) {
        if self.batch.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.batch);
        self.sent += batch.len();
        // A closed receiver means the dialog is gone – treat it like a cancel.
        if !self.channel.send(batch) {
            self.disconnected = true;
        }
    }

    fn flow(&self) -> Flow {
        if self.truncated || self.disconnected {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

/// Searches a file for the first matching line (content search).
/// Returns None if the file is binary/too large or nothing matches.
fn content_match(path: &Path, size: u64, needle: &Needle) -> std::io::Result<Option<String>> {
    if size > MAX_CONTENT_BYTES {
        return Ok(None);
    }
    let mut buf = Vec::new();
    fs::File::open(path)?.read_to_end(&mut buf)?;
    if buf.contains(&0) {
        return Ok(None); // binär
    }
    let Ok(text) = std::str::from_utf8(&buf) else {
        return Ok(None);
    };
    for (i, line) in text.lines().enumerate() {
        if needle.is_match(line) {
            let trimmed: String = line.trim().chars().take(200).collect();
            return Ok(Some(format!("{}: {}", i + 1, trimmed)));
        }
    }
    Ok(None)
}

/// Content matcher: regex or (case-sensitive/-insensitive) substring.
enum Needle {
    Re(regex::Regex),
    Plain { text: String, case_sensitive: bool },
}

impl Needle {
    fn build(opts: &SearchOptions) -> AppResult<Option<Self>> {
        if opts.content.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(if opts.content_regex {
            Needle::Re(build_regex(&opts.content, opts.case_sensitive)?)
        } else {
            Needle::Plain {
                text: opts.content.clone(),
                case_sensitive: opts.case_sensitive,
            }
        }))
    }

    fn is_match(&self, line: &str) -> bool {
        match self {
            Needle::Re(re) => re.is_match(line),
            Needle::Plain {
                text,
                case_sensitive,
            } => {
                if *case_sensitive {
                    line.contains(text)
                } else {
                    line.to_lowercase().contains(&text.to_lowercase())
                }
            }
        }
    }
}

/// SHA-256 of a file (for duplicate detection).
fn hash_file(path: &Path) -> std::io::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_vec())
}

fn walk_opts(opts: &SearchOptions, dirs_only: bool) -> WalkOptions {
    WalkOptions {
        ignore_dirs: opts.ignore_dirs.clone(),
        want_files: !dirs_only,
        want_dirs: dirs_only,
        detect_empty_dirs: dirs_only,
        threads: WALK_THREADS,
    }
}

pub(crate) fn run_search(
    root: PathBuf,
    opts: SearchOptions,
    cancel_flag: Arc<AtomicBool>,
    limits: &Limits,
    channel: &dyn HitSink,
) -> AppResult<SearchSummary> {
    let name_match = NameMatch::build(&opts)?;
    let needle = Needle::build(&opts)?;
    let mut sink = Sink::new(channel, limits);
    let mut limited = false;
    let mut unreadable = 0u64;

    let mut report = match opts.mode.as_str() {
        "empty_dirs" => walk_local(&root, &walk_opts(&opts, true), &cancel_flag, |entry| {
            if !entry.is_dir || !entry.dir_empty {
                return Flow::Continue;
            }
            let mut h = hit(&entry.path, entry.size, entry.modified, String::new(), 0);
            h.is_dir = entry.is_dir;
            sink.push(h);
            sink.flow()
        }),

        "large_files" => {
            // Bounded max-heap: only the biggest `large_candidates`
            // files are kept, so sorting never buffers the whole tree.
            let mut heap: BinaryHeap<Reverse<(u64, u64, PathBuf)>> = BinaryHeap::new();
            let report = walk_local(&root, &walk_opts(&opts, false), &cancel_flag, |entry| {
                if entry.size < opts.min_size {
                    return Flow::Continue;
                }
                heap.push(Reverse((entry.size, entry.modified, entry.path)));
                if heap.len() > limits.large_candidates {
                    heap.pop(); // kleinste Kandidatin fällt raus
                    limited = true;
                }
                Flow::Continue
            });
            let mut sorted: Vec<_> = heap.into_iter().map(|Reverse(t)| t).collect();
            sorted.sort_by_key(|t| Reverse(t.0)); // größte zuerst
            for (size, modified, path) in sorted {
                if !sink.push(hit(&path, size, modified, String::new(), 0)) {
                    break;
                }
            }
            report
        }

        "duplicates" => {
            let mut by_size: HashMap<u64, Vec<(PathBuf, u64)>> = HashMap::new();
            let mut candidates = 0usize;
            let report = walk_local(&root, &walk_opts(&opts, false), &cancel_flag, |entry| {
                if entry.size == 0 {
                    return Flow::Continue;
                }
                if candidates >= limits.dup_candidates {
                    limited = true;
                    return Flow::Stop;
                }
                candidates += 1;
                by_size
                    .entry(entry.size)
                    .or_default()
                    .push((entry.path, entry.modified));
                Flow::Continue
            });
            find_duplicates(by_size, &cancel_flag, &mut sink, &mut unreadable);
            report
        }

        // Default: name and/or content search.
        _ => walk_local(&root, &walk_opts(&opts, false), &cancel_flag, |entry| {
            if !name_match.matches(&entry.name) {
                return Flow::Continue;
            }
            let detail = match &needle {
                Some(nd) => match content_match(&entry.path, entry.size, nd) {
                    Ok(Some(line)) => line,
                    Ok(None) => return Flow::Continue,
                    Err(_) => {
                        unreadable += 1;
                        return Flow::Continue;
                    }
                },
                None => String::new(),
            };
            sink.push(hit(&entry.path, entry.size, entry.modified, detail, 0));
            sink.flow()
        }),
    };

    report.unreadable += unreadable;
    sink.flush();
    let cancelled = report.cancelled || sink.disconnected || cancel_flag.load(Ordering::Relaxed);
    let status = if cancelled {
        SearchStatus::Cancelled
    } else if sink.truncated {
        SearchStatus::Truncated
    } else if limited {
        SearchStatus::Limited
    } else if report.unreadable > 0 {
        SearchStatus::Partial
    } else {
        SearchStatus::Complete
    };
    Ok(SearchSummary {
        status,
        hits: sink.sent as u64,
        files: report.files,
        dirs: report.dirs,
        unreadable: report.unreadable,
    })
}

/// Finds duplicates: pre-group by size, then within the same size
/// by SHA-256. Only groups with ≥2 identical files are reported.
fn find_duplicates(
    by_size: HashMap<u64, Vec<(PathBuf, u64)>>,
    cancel_flag: &AtomicBool,
    sink: &mut Sink,
    unreadable: &mut u64,
) {
    let mut group_id = 0u32;
    for (size, group) in by_size {
        if group.len() < 2 || cancel_flag.load(Ordering::Relaxed) {
            continue;
        }
        // Group further by hash within the same size.
        let mut by_hash: HashMap<Vec<u8>, Vec<(PathBuf, u64)>> = HashMap::new();
        for (p, mt) in group {
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            match hash_file(&p) {
                Ok(h) => by_hash.entry(h).or_default().push((p, mt)),
                Err(_) => *unreadable += 1,
            }
        }
        for dups in by_hash.into_values() {
            if dups.len() < 2 {
                continue;
            }
            group_id += 1;
            let detail = format!("{} × {}", dups.len(), super::human_size(size));
            for (p, mt) in &dups {
                if !sink.push(hit(p, size, *mt, detail.clone(), group_id)) {
                    return;
                }
            }
        }
    }
}

/// Starts the search and streams the matches over `on_batch`.
/// `token` identifies the run for `cancel_search`.
#[tauri::command]
pub async fn search(
    root: String,
    options: SearchOptions,
    token: String,
    on_batch: Channel<Vec<SearchHit>>,
) -> AppResult<SearchSummary> {
    // The traversal is the local jwalk engine (TICKET-013); a remote
    // root would be walked as a relative local path (TICKET-031).
    caps::require(&root, Capability::Search)?;
    let root_path = PathBuf::from(&root);
    // Register first, so every path out of here also unregisters again.
    let cancel_flag = cancel::register(KIND, &token);
    let result = if root_path.is_dir() {
        tauri::async_runtime::spawn_blocking(move || {
            run_search(root_path, options, cancel_flag, &Limits::DEFAULT, &on_batch)
        })
        .await
        .map_err(AppError::from)?
    } else {
        Err(AppError::from(format!("{}: kein Verzeichnis", root)))
    };
    cancel::unregister(KIND, &token);
    result
}

/// Stops a running search. Also works before the task has registered.
#[tauri::command]
pub fn cancel_search(token: String) {
    cancel::cancel(KIND, &token);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rc-search-{}-{}-{}",
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

    #[test]
    fn vanished_duplicate_candidate_is_counted_after_enumeration() {
        let root = tmp("vanished-duplicate");
        let a = root.join("a");
        let b = root.join("b");
        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();
        let candidates = HashMap::from([(4, vec![(a, 0), (b.clone(), 0)])]);
        fs::remove_file(b).unwrap();
        let channel = Channel::new(|_| Ok(()));
        let mut sink = Sink::new(&channel, &Limits::DEFAULT);
        let mut unreadable = 0;
        find_duplicates(
            candidates,
            &AtomicBool::new(false),
            &mut sink,
            &mut unreadable,
        );
        assert_eq!(unreadable, 1);
        assert_eq!(sink.total(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_read_errors_are_partial_but_binary_and_nonmatches_are_not() {
        let root = tmp("unreadable-content");
        let file = root.join("file");
        fs::write(&file, b"needle").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut options = opts("files");
            options.content = "needle".into();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o0)).unwrap();
            if fs::File::open(&file).is_err() {
                let (summary, hits) = run(&root, options);
                assert_eq!(summary.status, SearchStatus::Partial);
                assert_eq!(summary.unreadable, 1);
                assert!(hits.is_empty());
            }
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let needle = Needle::Plain {
            text: "needle".into(),
            case_sensitive: true,
        };
        fs::remove_file(&file).unwrap();
        assert!(content_match(&file, 6, &needle).is_err());
        for bytes in [b"other".as_slice(), b"\0needle", &[0xff]] {
            fs::write(&file, bytes).unwrap();
            assert!(content_match(&file, bytes.len() as u64, &needle)
                .unwrap()
                .is_none());
        }
        fs::remove_dir_all(root).unwrap();
    }

    fn opts(mode: &str) -> SearchOptions {
        SearchOptions {
            mode: mode.into(),
            name: String::new(),
            name_regex: false,
            content: String::new(),
            content_regex: false,
            case_sensitive: false,
            ignore_dirs: vec![],
            min_size: 0,
        }
    }

    /// Runs a search and collects everything the channel received.
    fn run(root: &Path, options: SearchOptions) -> (SearchSummary, Vec<SearchHit>) {
        run_with(root, options, false, &Limits::DEFAULT, true)
    }

    fn run_cancelled(
        root: &Path,
        options: SearchOptions,
        cancelled: bool,
    ) -> (SearchSummary, Vec<SearchHit>) {
        run_with(root, options, cancelled, &Limits::DEFAULT, true)
    }

    fn run_with(
        root: &Path,
        options: SearchOptions,
        cancelled: bool,
        limits: &Limits,
        channel_open: bool,
    ) -> (SearchSummary, Vec<SearchHit>) {
        use std::sync::Mutex;
        // Each run gets its own collector – the tests run in parallel.
        let collected: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let channel = Channel::new(move |msg| {
            // The payload only arrives serialized here, so the hits are
            // parsed back from JSON below.
            if let tauri::ipc::InvokeResponseBody::Json(json) = msg {
                sink.lock().unwrap().push(json);
            }
            if channel_open {
                Ok(())
            } else {
                // Simulates a dialog that is gone: the send fails.
                Err(tauri::Error::UnknownPath)
            }
        });
        let flag = Arc::new(AtomicBool::new(cancelled));
        let summary = run_search(root.to_path_buf(), options, flag, limits, &channel).unwrap();
        drop(channel);
        let hits = collected
            .lock()
            .unwrap()
            .iter()
            .flat_map(|json| serde_json::from_str::<Vec<SearchHit>>(json).unwrap())
            .collect();
        (summary, hits)
    }

    impl<'de> serde::Deserialize<'de> for SearchHit {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            struct Raw {
                path: String,
                name: String,
                is_dir: bool,
                size: u64,
                modified: u64,
                detail: String,
                group: u32,
            }
            let r = Raw::deserialize(d)?;
            Ok(SearchHit {
                path: r.path,
                name: r.name,
                is_dir: r.is_dir,
                size: r.size,
                modified: r.modified,
                detail: r.detail,
                group: r.group,
            })
        }
    }

    #[test]
    fn glob_matches_extension() {
        let re = glob_to_regex("*.txt *.md", false).unwrap();
        assert!(re.is_match("a.TXT"));
        assert!(re.is_match("readme.md"));
        assert!(!re.is_match("a.rs"));
    }

    #[test]
    fn name_and_content_needle() {
        let plain = Needle::Plain {
            text: "Fehler".into(),
            case_sensitive: false,
        };
        assert!(plain.is_match("ein FEHLER hier"));
        assert!(!plain.is_match("alles gut"));
        let re = Needle::Re(build_regex(r"TODO:\s*\w+", true).unwrap());
        assert!(re.is_match("  TODO: fixme"));
    }

    /// One fixture for all four modes: nested folders, an ignored name,
    /// an empty folder, duplicates and files with content.
    fn fixture(name: &str) -> PathBuf {
        let d = tmp(name);
        fs::create_dir_all(d.join("sub/deeper")).unwrap();
        fs::create_dir_all(d.join("leer")).unwrap();
        fs::create_dir_all(d.join("node_modules/inner")).unwrap();
        fs::write(d.join("node_modules/pkg.js"), b"TODO: ignoriert").unwrap();
        fs::write(d.join("a.txt"), b"erste Zeile\nTODO: aufraeumen\n").unwrap();
        fs::write(d.join("sub/b.txt"), b"nichts hier\n").unwrap();
        fs::write(d.join("sub/deeper/c.log"), vec![b'x'; 4096]).unwrap();
        // Duplicate pair (identical content), plus a same-size decoy.
        fs::write(d.join("dup1.bin"), b"gleicher inhalt!").unwrap();
        fs::write(d.join("sub/dup2.bin"), b"gleicher inhalt!").unwrap();
        fs::write(d.join("sub/other.bin"), b"andrer inhalt!!!").unwrap();
        d
    }

    #[test]
    fn files_mode_matches_name_and_ignores_pruned_dirs() {
        let d = fixture("files");
        let mut o = opts("files");
        o.name = "*.txt".into();
        o.ignore_dirs = vec!["node_modules".into()];
        let (summary, hits) = run(&d, o);
        let mut names: Vec<_> = hits.iter().map(|h| h.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
        assert_eq!(summary.status, SearchStatus::Complete);
        assert_eq!(summary.hits, 2);
        // Six files in the fixture; the pruned `node_modules/pkg.js`
        // was never even read.
        assert_eq!(summary.files, 6);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn files_mode_content_search() {
        let d = fixture("content");
        let mut o = opts("files");
        o.content = "TODO".into();
        o.ignore_dirs = vec!["node_modules".into()];
        let (_, hits) = run(&d, o);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "a.txt");
        assert!(hits[0].detail.starts_with("2: TODO"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_dirs_mode() {
        let d = fixture("empty");
        let mut o = opts("empty_dirs");
        o.ignore_dirs = vec!["node_modules".into()];
        let (summary, hits) = run(&d, o);
        let names: Vec<_> = hits.iter().map(|h| h.name.clone()).collect();
        assert_eq!(names, vec!["leer"]);
        assert!(hits[0].is_dir);
        assert_eq!(summary.status, SearchStatus::Complete);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn large_files_mode_sorts_biggest_first() {
        let d = fixture("large");
        let mut o = opts("large_files");
        o.min_size = 1024;
        o.ignore_dirs = vec!["node_modules".into()];
        let (_, hits) = run(&d, o);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "c.log");
        assert_eq!(hits[0].size, 4096);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn duplicates_mode_groups_identical_files() {
        let d = fixture("dups");
        let mut o = opts("duplicates");
        o.ignore_dirs = vec!["node_modules".into()];
        let (_, hits) = run(&d, o);
        let mut names: Vec<_> = hits.iter().map(|h| h.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["dup1.bin", "dup2.bin"]);
        // Same size but different content must not be grouped.
        assert!(hits.iter().all(|h| h.group == 1));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cancelled_run_is_not_reported_complete() {
        let d = fixture("cancel");
        let (summary, hits) = run_cancelled(&d, opts("files"), true);
        assert_eq!(summary.status, SearchStatus::Cancelled);
        assert!(hits.is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_terminates_and_links_are_not_hits() {
        let d = fixture("links");
        std::os::unix::fs::symlink(&d, d.join("sub/back")).unwrap();
        std::os::unix::fs::symlink(d.join("a.txt"), d.join("link.txt")).unwrap();
        let mut o = opts("files");
        o.name = "*.txt".into();
        o.ignore_dirs = vec!["node_modules".into()];
        let (summary, hits) = run(&d, o);
        let mut names: Vec<_> = hits.iter().map(|h| h.name.clone()).collect();
        names.sort();
        // link.txt is a symlink → not reported; the loop terminates.
        assert_eq!(names, vec!["a.txt", "b.txt"]);
        assert_eq!(summary.status, SearchStatus::Complete);
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folder_makes_the_scan_partial() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("unreadable");
        fs::write(d.join("a.txt"), b"hi").unwrap();
        let locked = d.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("hidden.txt"), b"hi").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let (summary, _) = run(&d, opts("files"));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        // Running as root ignores the permission bits – then nothing is
        // skipped and the scan is legitimately complete.
        if summary.unreadable > 0 {
            assert_eq!(summary.status, SearchStatus::Partial);
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// Small limits so the limit paths can be tested without building
    /// fixtures with tens of thousands of files.
    fn tiny_limits() -> Limits {
        Limits {
            max_hits: 3,
            batch: 2,
            dup_candidates: 2,
            large_candidates: 2,
        }
    }

    #[test]
    fn hit_limit_truncates_and_stops_the_walk() {
        let d = tmp("limit");
        for i in 0..10 {
            fs::write(d.join(format!("f{}.txt", i)), b"x").unwrap();
        }
        let (summary, hits) = run_with(&d, opts("files"), false, &tiny_limits(), true);
        assert_eq!(summary.status, SearchStatus::Truncated);
        assert_eq!(hits.len(), 3);
        assert_eq!(summary.hits, 3);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn large_file_heap_keeps_the_biggest_and_reports_limited() {
        let d = tmp("heap");
        for (i, size) in [10usize, 40, 20, 30].iter().enumerate() {
            fs::write(d.join(format!("f{}.bin", i)), vec![0u8; *size]).unwrap();
        }
        let (summary, hits) = run_with(&d, opts("large_files"), false, &tiny_limits(), true);
        // Only the two biggest survive the bounded heap, biggest first.
        let sizes: Vec<u64> = hits.iter().map(|h| h.size).collect();
        assert_eq!(sizes, vec![40, 30]);
        assert_eq!(summary.status, SearchStatus::Limited);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn duplicate_candidate_limit_reports_limited() {
        let d = tmp("duplimit");
        for i in 0..6 {
            fs::write(d.join(format!("f{}.bin", i)), b"same").unwrap();
        }
        let (summary, _) = run_with(&d, opts("duplicates"), false, &tiny_limits(), true);
        assert_eq!(summary.status, SearchStatus::Limited);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn closed_channel_ends_the_search() {
        let d = tmp("closed");
        for i in 0..10 {
            fs::write(d.join(format!("f{}.txt", i)), b"x").unwrap();
        }
        let (summary, _) = run_with(&d, opts("files"), false, &tiny_limits(), false);
        // A dialog that is gone must not leave the scan running, and the
        // result must not look complete.
        assert_eq!(summary.status, SearchStatus::Cancelled);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn deep_tree_is_searched_without_growing_the_stack() {
        let d = tmp("deep");
        let mut cur = d.clone();
        // One-character names: the depth is capped by PATH_MAX.
        for _ in 0..400 {
            cur = cur.join("d");
            fs::create_dir_all(&cur).unwrap();
        }
        fs::write(cur.join("tief.txt"), b"x").unwrap();
        let mut o = opts("files");
        o.name = "tief.txt".into();
        let (summary, hits) = run(&d, o);
        assert_eq!(hits.len(), 1);
        assert_eq!(summary.dirs, 400);
        assert_eq!(summary.status, SearchStatus::Complete);
        let _ = fs::remove_dir_all(&d);
    }
}
