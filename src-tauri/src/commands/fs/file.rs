// ============================================================
// File operations: copy/move/delete/create folder.
// Contains the shared transfer infrastructure (progress,
// cancel, name conflicts) that the archive module also uses.
// ============================================================

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

/// Ongoing progress of a transfer – event "fs-progress".
/// Two levels: current file (file_done/file_total) and
/// overall operation (bytes_done/bytes_total, files_done/files_total).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpProgress {
    /// Unique operation ID (assigned by the frontend).
    pub id: String,
    /// "copy" | "move" | "extract" | "pack"
    pub op: String,
    /// currently processed file (full path)
    pub file_name: String,
    #[ts(type = "number")]
    pub file_done: u64,
    #[ts(type = "number")]
    pub file_total: u64,
    pub files_done: u32,
    pub files_total: u32,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
    /// Whether the operation is currently paused (waiting to resume).
    pub paused: bool,
}

/// Completion of a transfer – event "fs-done".
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpDone {
    pub id: String,
    pub op: String,
    pub ok: u32,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

/// Name-conflict request – event "fs-collision".
/// The transfer thread waits until the frontend calls `resolve_collision`.
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CollisionReq {
    pub transfer_id: String,
    pub req_id: String,
    /// target path that already exists
    pub path: String,
    pub is_dir: bool,
}

/// Result of a synchronous operation (e.g. delete).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpResult {
    pub ok: u32,
    pub errors: Vec<String>,
}

/// Builds the target path in the target folder from the source base name.
fn target_in(dest_dir: &Path, src: &Path) -> Option<PathBuf> {
    src.file_name().map(|name| dest_dir.join(name))
}

/// Counts files and bytes recursively (for the progress totals).
fn measure(p: &Path) -> (u32, u64) {
    if p.is_dir() {
        match fs::read_dir(p) {
            Ok(rd) => rd.flatten().fold((0, 0), |(f, b), e| {
                let (cf, cb) = measure(&e.path());
                (f + cf, b + cb)
            }),
            Err(_) => (0, 0),
        }
    } else {
        let size = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        (1, size)
    }
}

/// Resolution of a name conflict.
#[derive(Clone, Copy)]
enum Resolution {
    Overwrite,
    Rename,
    Skip,
}

// ----- Operation registries (cancel flags + collision channels) -----

type CancelMap = Mutex<HashMap<String, Arc<AtomicBool>>>;
static CANCELS: OnceLock<CancelMap> = OnceLock::new();
fn cancels() -> &'static CancelMap {
    CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

// Pause flags per operation (true = paused). The copy loop waits on them.
static PAUSES: OnceLock<CancelMap> = OnceLock::new();
fn pauses() -> &'static CancelMap {
    PAUSES.get_or_init(|| Mutex::new(HashMap::new()))
}

type ResolverMap = Mutex<HashMap<String, mpsc::Sender<(Resolution, bool)>>>;
static RESOLVERS: OnceLock<ResolverMap> = OnceLock::new();
fn resolvers() -> &'static ResolverMap {
    RESOLVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

static REQ_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Finds a free name "Name (2)", "Name (3)" … in the target folder.
fn free_name(target: &Path) -> PathBuf {
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = target
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut i = 2u32;
    loop {
        let candidate = parent.join(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

/// Asks the frontend for the conflict resolution and blocks until the answer.
/// Reacts to cancel (skip) so the thread never hangs.
fn ask_collision(
    app: &AppHandle,
    transfer_id: &str,
    target: &Path,
    cancel: &Arc<AtomicBool>,
) -> (Resolution, bool) {
    let req_id = format!("c{}", REQ_COUNTER.fetch_add(1, Ordering::Relaxed));
    let (tx, rx) = mpsc::channel::<(Resolution, bool)>();
    resolvers().lock().unwrap().insert(req_id.clone(), tx);

    let _ = app.emit(
        "fs-collision",
        CollisionReq {
            transfer_id: transfer_id.to_string(),
            req_id: req_id.clone(),
            path: target.to_string_lossy().into_owned(),
            is_dir: target.is_dir(),
        },
    );

    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break (Resolution::Skip, false);
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(v) => break v,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break (Resolution::Skip, false),
        }
    };
    resolvers().lock().unwrap().remove(&req_id);
    result
}

/// Holds the cumulative counters + cancel flag and sends throttled events.
/// Also used by the archive module (extract/pack).
pub(crate) struct Prog {
    pub(crate) app: AppHandle,
    pub(crate) id: String,
    pub(crate) op: String,
    pub(crate) files_total: u32,
    pub(crate) bytes_total: u64,
    pub(crate) files_done: u32,
    pub(crate) bytes_done: u64,
    pub(crate) last: Instant,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) pause: Arc<AtomicBool>,
}

impl Prog {
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn emit(&mut self, file_name: &str, file_done: u64, file_total: u64, force: bool) {
        self.emit_state(file_name, file_done, file_total, force, false);
    }

    fn emit_state(
        &mut self,
        file_name: &str,
        file_done: u64,
        file_total: u64,
        force: bool,
        paused: bool,
    ) {
        // Don't send more often than every 40 ms (exception: forced ticks).
        if !force && self.last.elapsed() < Duration::from_millis(40) {
            return;
        }
        self.last = Instant::now();
        let _ = self.app.emit(
            "fs-progress",
            OpProgress {
                id: self.id.clone(),
                op: self.op.clone(),
                file_name: file_name.to_string(),
                file_done,
                file_total,
                files_done: self.files_done,
                files_total: self.files_total,
                bytes_done: self.bytes_done,
                bytes_total: self.bytes_total,
                paused,
            },
        );
    }

    /// Blocks while the operation is paused (and not cancelled),
    /// keeping the UI informed via throttled "paused" ticks.
    /// Resets the throttle/rate reference after resuming.
    pub(crate) fn wait_if_paused(&mut self, file_name: &str, file_done: u64, file_total: u64) {
        if !self.pause.load(Ordering::Relaxed) {
            return;
        }
        while self.pause.load(Ordering::Relaxed) && !self.cancelled() {
            self.emit_state(file_name, file_done, file_total, true, true);
            std::thread::sleep(Duration::from_millis(150));
        }
        self.last = Instant::now();
    }
}

/// Copies a file in blocks; reacts to cancel (the partial file is
/// removed) and pause. `limit` optionally caps the speed
/// (bytes/s, 0 = unlimited). With `verify`, the SHA-256 checksum of
/// source and target is compared after copying.
fn copy_file_prog(
    src: &Path,
    dst: &Path,
    prog: &mut Prog,
    limit: u64,
    verify: bool,
    buf_size: usize,
) -> std::io::Result<()> {
    use sha2::{Digest, Sha256};

    let file_total = fs::metadata(src)?.len();
    let name = src.to_string_lossy().into_owned();
    let mut reader = File::open(src)?;
    let mut writer = File::create(dst)?;
    let mut buf = vec![0u8; buf_size];
    let mut file_done = 0u64;
    let mut hasher = verify.then(Sha256::new);

    // Rate-limit window (only for limit > 0): reset roughly every 1 s.
    let mut win_start = Instant::now();
    let mut win_bytes = 0u64;

    let abort = |writer: File, dst: &Path| {
        drop(writer);
        let _ = fs::remove_file(dst); // unvollständige Zieldatei verwerfen
        std::io::Error::new(ErrorKind::Interrupted, "abgebrochen")
    };

    prog.emit(&name, 0, file_total, true);
    loop {
        if prog.cancelled() {
            return Err(abort(writer, dst));
        }
        // Block while paused; afterwards check for cancel again if needed.
        prog.wait_if_paused(&name, file_done, file_total);
        if prog.cancelled() {
            return Err(abort(writer, dst));
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        file_done += n as u64;
        prog.bytes_done += n as u64;
        prog.emit(&name, file_done, file_total, false);

        // Speed limit: sleep briefly when exceeded.
        if limit > 0 {
            win_bytes += n as u64;
            let elapsed = win_start.elapsed().as_secs_f64();
            let allowed = limit as f64 * elapsed;
            if win_bytes as f64 > allowed {
                let wait = (win_bytes as f64 - allowed) / limit as f64;
                std::thread::sleep(Duration::from_secs_f64(wait.min(1.0)));
            }
            if elapsed >= 1.0 {
                win_start = Instant::now();
                win_bytes = 0;
            }
        }
    }

    // Verification: read the target again and compare checksums.
    if let Some(h) = hasher {
        let src_hash = h.finalize();
        let dst_hash = hash_file_sha256(dst)?;
        if src_hash.as_slice() != dst_hash.as_slice() {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Prüfsumme stimmt nicht (Kopie beschädigt)",
            ));
        }
    }

    prog.files_done += 1;
    prog.emit(&name, file_total, file_total, true);
    Ok(())
}

/// SHA-256 of a file (for the copy verification).
fn hash_file_sha256(path: &Path) -> std::io::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let mut f = File::open(path)?;
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

/// Copies recursively (file or directory tree).
fn copy_tree(
    src: &Path,
    dst: &Path,
    prog: &mut Prog,
    limit: u64,
    verify: bool,
    buf_size: usize,
) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()), prog, limit, verify, buf_size)?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_file_prog(src, dst, prog, limit, verify, buf_size)?;
    }
    Ok(())
}

/// Resolves a buffer size in KB into a safe byte count
/// (0 = default 256 KB; otherwise 4 KB … 16 MB).
fn resolve_buf(buf_kb: u64) -> usize {
    if buf_kb == 0 {
        256 * 1024
    } else {
        (buf_kb.clamp(4, 16 * 1024) as usize) * 1024
    }
}

// ============================================================
// Parallel copy path (only for thread count > 1 and pure copying).
// Deliberately isolated from the sequential path: `SharedProg` keeps the
// progress in atomics, so several worker threads can safely share
// the same bookkeeping. Move and archive stay untouched.
// ============================================================

/// Thread-safe progress for the parallel copy path.
struct SharedProg {
    app: AppHandle,
    id: String,
    op: String,
    files_total: u32,
    bytes_total: u64,
    bytes_done: AtomicU64,
    files_done: AtomicU32,
    last: Mutex<Instant>,
    rate: Mutex<(Instant, u64)>, // (Fensterbeginn, Bytes im Fenster)
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
}

impl SharedProg {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn emit(&self, file_name: &str, file_done: u64, file_total: u64, force: bool, paused: bool) {
        {
            let mut last = self.last.lock().unwrap();
            if !force && last.elapsed() < Duration::from_millis(40) {
                return;
            }
            *last = Instant::now();
        }
        let _ = self.app.emit(
            "fs-progress",
            OpProgress {
                id: self.id.clone(),
                op: self.op.clone(),
                file_name: file_name.to_string(),
                file_done,
                file_total,
                files_done: self.files_done.load(Ordering::Relaxed),
                files_total: self.files_total,
                bytes_done: self.bytes_done.load(Ordering::Relaxed),
                bytes_total: self.bytes_total,
                paused,
            },
        );
    }

    /// Blocks while paused (and not cancelled); afterwards resets
    /// the rate window so the throttle does not catch up.
    fn wait_if_paused(&self, name: &str, file_done: u64, file_total: u64) {
        if !self.pause.load(Ordering::Relaxed) {
            return;
        }
        while self.pause.load(Ordering::Relaxed) && !self.cancelled() {
            self.emit(name, file_done, file_total, true, true);
            std::thread::sleep(Duration::from_millis(150));
        }
        if let Ok(mut g) = self.rate.lock() {
            *g = (Instant::now(), 0);
        }
    }

    /// Global speed limit across all workers (limit = bytes/s).
    fn rate_limit(&self, n: usize, limit: u64) {
        if limit == 0 {
            return;
        }
        let sleep_secs;
        {
            let mut g = self.rate.lock().unwrap();
            g.1 += n as u64;
            let elapsed = g.0.elapsed().as_secs_f64();
            let allowed = limit as f64 * elapsed;
            sleep_secs = if g.1 as f64 > allowed {
                (g.1 as f64 - allowed) / limit as f64
            } else {
                0.0
            };
            if elapsed >= 1.0 {
                *g = (Instant::now(), 0);
            }
        }
        if sleep_secs > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(sleep_secs.min(1.0)));
        }
    }
}

/// A single file copy task (with source index for the ok counting).
struct CopyTask {
    idx: usize,
    src: PathBuf,
    dst: PathBuf,
}

/// Flattens a source tree into individual file tasks, creating the
/// target folders along the way. `idx` refers to the top-level source.
fn flatten_tree(src: &Path, dst: &Path, idx: usize, tasks: &mut Vec<CopyTask>) {
    if src.is_dir() {
        let _ = fs::create_dir_all(dst);
        if let Ok(rd) = fs::read_dir(src) {
            for e in rd.flatten() {
                flatten_tree(&e.path(), &dst.join(e.file_name()), idx, tasks);
            }
        }
    } else {
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }
        tasks.push(CopyTask {
            idx,
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
        });
    }
}

/// Copies a file block by block with shared bookkeeping (parallel path).
fn parallel_copy_file(
    task: &CopyTask,
    sp: &SharedProg,
    limit: u64,
    verify: bool,
    buf_size: usize,
) -> std::io::Result<()> {
    use sha2::{Digest, Sha256};

    let file_total = fs::metadata(&task.src)?.len();
    let name = task.src.to_string_lossy().into_owned();
    let mut reader = File::open(&task.src)?;
    let mut writer = File::create(&task.dst)?;
    let mut buf = vec![0u8; buf_size];
    let mut hasher = verify.then(Sha256::new);
    let mut file_done = 0u64;

    let abort = |writer: File, dst: &Path| {
        drop(writer);
        let _ = fs::remove_file(dst);
        std::io::Error::new(ErrorKind::Interrupted, "abgebrochen")
    };

    sp.emit(&name, 0, file_total, true, false);
    loop {
        if sp.cancelled() {
            return Err(abort(writer, &task.dst));
        }
        sp.wait_if_paused(&name, file_done, file_total);
        if sp.cancelled() {
            return Err(abort(writer, &task.dst));
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        file_done += n as u64;
        sp.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
        sp.emit(&name, file_done, file_total, false, false);
        sp.rate_limit(n, limit);
    }

    if let Some(h) = hasher {
        let src_hash = h.finalize();
        let dst_hash = hash_file_sha256(&task.dst)?;
        if src_hash.as_slice() != dst_hash.as_slice() {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Prüfsumme stimmt nicht (Kopie beschädigt)",
            ));
        }
    }

    sp.files_done.fetch_add(1, Ordering::Relaxed);
    sp.emit(&name, file_total, file_total, true, false);
    Ok(())
}

/// Copies the resolved (source, target) pairs in parallel with `threads`
/// worker threads. Returns: (successful sources, error messages).
#[allow(clippy::too_many_arguments)]
fn run_parallel_copy(
    app: AppHandle,
    id: String,
    op: String,
    jobs: Vec<(PathBuf, PathBuf)>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    limit: u64,
    verify: bool,
    buf_size: usize,
    threads: u32,
    files_total: u32,
    bytes_total: u64,
) -> (u32, Vec<String>) {
    // Flatten all source trees into file tasks (target folders are created).
    let mut tasks = Vec::new();
    for (i, (src, target)) in jobs.iter().enumerate() {
        flatten_tree(src, target, i, &mut tasks);
    }

    let sp = Arc::new(SharedProg {
        app,
        id,
        op,
        files_total,
        bytes_total,
        bytes_done: AtomicU64::new(0),
        files_done: AtomicU32::new(0),
        last: Mutex::new(Instant::now()),
        rate: Mutex::new((Instant::now(), 0)),
        cancel,
        pause,
    });
    let tasks = Arc::new(tasks);
    let next = Arc::new(AtomicUsize::new(0));
    let failed: Arc<Vec<AtomicBool>> =
        Arc::new((0..jobs.len()).map(|_| AtomicBool::new(false)).collect());
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let n_threads = (threads.clamp(2, 16) as usize).min(tasks.len().max(1));
    let handles: Vec<_> = (0..n_threads)
        .map(|_| {
            let (sp, tasks, next, failed, errors) = (
                sp.clone(),
                tasks.clone(),
                next.clone(),
                failed.clone(),
                errors.clone(),
            );
            std::thread::spawn(move || loop {
                if sp.cancelled() {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= tasks.len() {
                    break;
                }
                let task = &tasks[i];
                match parallel_copy_file(task, &*sp, limit, verify, buf_size) {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::Interrupted => break, // Abbruch
                    Err(e) => {
                        failed[task.idx].store(true, Ordering::Relaxed);
                        errors
                            .lock()
                            .unwrap()
                            .push(format!("{}: {}", task.src.display(), e));
                    }
                }
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }

    let ok = (0..jobs.len())
        .filter(|i| !failed[*i].load(Ordering::Relaxed))
        .count() as u32;
    let errors = Arc::try_unwrap(errors)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    (ok, errors)
}

/// Runs a copy/move operation (runs in its own thread).
#[allow(clippy::too_many_arguments)]
fn run_transfer(
    app: AppHandle,
    id: String,
    op: String,
    sources: Vec<String>,
    dest_dir: String,
    remove_source: bool,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    limit: u64,
    verify: bool,
    buf_kb: u64,
    threads: u32,
) {
    let dest = PathBuf::from(&dest_dir);
    let buf_size = resolve_buf(buf_kb);
    // Parallel path only for pure copying and thread count > 1.
    // Move stays sequential (rename is immediate anyway; copy fallback is rare).
    let parallel = !remove_source && threads > 1;
    // Measure per source up front (totals + bookkeeping for a rename move).
    let measured: Vec<(String, u32, u64)> = sources
        .iter()
        .map(|s| {
            let (f, b) = measure(Path::new(s));
            (s.clone(), f, b)
        })
        .collect();
    let files_total = measured.iter().map(|m| m.1).sum();
    let bytes_total = measured.iter().map(|m| m.2).sum();

    let mut prog = Prog {
        app: app.clone(),
        id: id.clone(),
        op: op.clone(),
        files_total,
        bytes_total,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: cancel.clone(),
        pause: pause.clone(),
    };
    prog.emit("", 0, 0, true); // initialer 0%-Tick

    let mut ok = 0u32;
    let mut errors: Vec<String> = Vec::new();
    let mut default_res: Option<Resolution> = None; // „Für alle übernehmen"
    // In parallel mode the resolved pairs are collected and afterwards
    // copied in parallel (collisions are resolved here sequentially/interactively).
    let mut copy_jobs: Vec<(PathBuf, PathBuf)> = Vec::new();

    'outer: for (s, files, bytes) in &measured {
        if prog.cancelled() {
            break;
        }
        let src = Path::new(s);
        let Some(mut target) = target_in(&dest, src) else {
            errors.push(format!("{}: ungültiger Name", s));
            continue;
        };
        if target == *src {
            errors.push(format!("{}: Quelle und Ziel sind identisch", s));
            continue;
        }

        // Name conflict: ask the frontend (or apply the remembered choice).
        if target.exists() {
            let res = match default_res {
                Some(r) => r,
                None => {
                    let (r, apply_all) = ask_collision(&app, &id, &target, &cancel);
                    if apply_all {
                        default_res = Some(r);
                    }
                    r
                }
            };
            match res {
                Resolution::Skip => continue,
                Resolution::Rename => target = free_name(&target),
                Resolution::Overwrite => {
                    let rm = if target.is_dir() {
                        fs::remove_dir_all(&target)
                    } else {
                        fs::remove_file(&target)
                    };
                    if let Err(e) = rm {
                        errors.push(format!("{}: {}", s, e));
                        continue;
                    }
                }
            }
        }

        // Parallel mode (copy only): remember the pair, actual copying happens later.
        if parallel {
            copy_jobs.push((src.to_path_buf(), target));
            continue;
        }

        let result = if remove_source {
            // Move: first rename (immediate); otherwise copy + delete the source.
            match fs::rename(src, &target) {
                Ok(_) => {
                    prog.files_done += files;
                    prog.bytes_done += bytes;
                    prog.emit(&src.to_string_lossy(), *bytes, *bytes, true);
                    Ok(())
                }
                Err(_) => copy_tree(src, &target, &mut prog, limit, verify, buf_size).and_then(|_| {
                    if src.is_dir() {
                        fs::remove_dir_all(src)
                    } else {
                        fs::remove_file(src)
                    }
                }),
            }
        } else {
            copy_tree(src, &target, &mut prog, limit, verify, buf_size)
        };

        match result {
            Ok(_) => ok += 1,
            Err(e) if e.kind() == ErrorKind::Interrupted => break 'outer,
            Err(e) => errors.push(format!("{}: {}", s, e)),
        }
    }

    // Parallel copy phase (all collisions are already resolved).
    if parallel && !copy_jobs.is_empty() && !prog.cancelled() {
        let (pok, perrors) = run_parallel_copy(
            app.clone(),
            id.clone(),
            op.clone(),
            copy_jobs,
            cancel.clone(),
            pause.clone(),
            limit,
            verify,
            buf_size,
            threads,
            files_total,
            bytes_total,
        );
        ok += pok;
        errors.extend(perrors);
    }

    let cancelled = prog.cancelled();
    let _ = app.emit(
        "fs-done",
        OpDone {
            id,
            op,
            ok,
            errors,
            cancelled,
        },
    );
}

/// Registers the cancel flag and starts a background operation.
/// Shared by transfers and archive operations.
pub(crate) fn spawn_op<F>(id: String, f: F)
where
    F: FnOnce(Arc<AtomicBool>, Arc<AtomicBool>) + Send + 'static,
{
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));
    cancels().lock().unwrap().insert(id.clone(), cancel.clone());
    pauses().lock().unwrap().insert(id.clone(), pause.clone());
    std::thread::spawn(move || {
        f(cancel, pause);
        cancels().lock().unwrap().remove(&id);
        pauses().lock().unwrap().remove(&id);
    });
}

/// Creates a directory (recursively).
#[tauri::command]
pub fn make_dir(path: String) -> Result<String, String> {
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Writes text into a file (e.g. an exported file list).
/// Without `overwrite`, an existing file is left untouched:
/// returning `Ok(false)` signals the name conflict to the frontend.
/// `Ok(true)` = written, `Err` = a real I/O error.
#[tauri::command]
pub fn write_text_file(
    path: String,
    contents: String,
    overwrite: bool,
) -> Result<bool, String> {
    if !overwrite && Path::new(&path).exists() {
        return Ok(false);
    }
    fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Renames an entry (file or folder) within the same folder.
/// Refuses to overwrite an already existing target name.
#[tauri::command]
pub fn rename_entry(from: String, to: String) -> Result<(), String> {
    let from_p = Path::new(&from);
    let to_p = Path::new(&to);
    if !from_p.exists() {
        return Err("Quelle existiert nicht".into());
    }
    if to_p.exists() {
        return Err("Zielname existiert bereits".into());
    }
    fs::rename(from_p, to_p).map_err(|e| e.to_string())
}

/// Finds a free temporary name in the same folder (hidden file).
fn unique_temp(base: &Path) -> PathBuf {
    loop {
        let n = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        let cand = base.join(format!(".rcren-tmp-{}", n));
        if !cand.exists() {
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
    let base = Path::new(&dir);
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
        if !base.join(from).exists() {
            errors.push(format!("{}: Quelle nicht gefunden", from));
            continue;
        }
        // Foreign collision: the target exists and is not itself being renamed.
        if base.join(to).exists() && !sources.contains(to.as_str()) {
            errors.push(format!("{} → {}: Ziel existiert bereits", from, to));
            continue;
        }
        valid.push((from.as_str(), to.as_str()));
    }

    // Phase 1: all sources to temporary names.
    let mut staged: Vec<(PathBuf, &str, &str)> = Vec::new(); // (temp, to, from)
    for (from, to) in &valid {
        let tmp = unique_temp(base);
        match fs::rename(base.join(from), &tmp) {
            Ok(_) => staged.push((tmp, to, from)),
            Err(e) => errors.push(format!("{}: {}", from, e)),
        }
    }

    // Phase 2: temporary names to the target names.
    let mut ok = 0u32;
    for (tmp, to, from) in staged {
        match fs::rename(&tmp, base.join(to)) {
            Ok(_) => ok += 1,
            Err(e) => {
                // Rename back so no temporary name is left behind.
                let _ = fs::rename(&tmp, base.join(from));
                errors.push(format!("{} → {}: {}", from, to, e));
            }
        }
    }

    OpResult { ok, errors }
}

/// Starts a copy operation in the background (returns immediately).
/// `limit` = bytes/s (0 = unlimited), `verify` = checksums after copying.
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
) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| {
        run_transfer(
            app, id2, "copy".into(), sources, dest_dir, false, cancel, pause, limit, verify,
            buf_kb, threads,
        )
    });
}

/// Starts a move operation in the background (returns immediately).
/// `threads` is deliberately ignored (move stays sequential).
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
) {
    let id2 = id.clone();
    spawn_op(id, move |cancel, pause| {
        run_transfer(
            app, id2, "move".into(), sources, dest_dir, true, cancel, pause, limit, verify,
            buf_kb, threads,
        )
    });
}

/// Cancels a running transfer (sets its cancel flag).
#[tauri::command]
pub fn cancel_transfer(id: String) {
    if let Some(flag) = cancels().lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    // If paused: release the pause so the waiting thread sees the cancel.
    if let Some(flag) = pauses().lock().unwrap().get(&id) {
        flag.store(false, Ordering::Relaxed);
    }
}

/// Pauses or resumes a running transfer.
#[tauri::command]
pub fn pause_transfer(id: String, paused: bool) {
    if let Some(flag) = pauses().lock().unwrap().get(&id) {
        flag.store(paused, Ordering::Relaxed);
    }
}

/// Answer to a name conflict from the frontend.
#[tauri::command]
pub fn resolve_collision(req_id: String, action: String, apply_all: bool) {
    let res = match action.as_str() {
        "overwrite" => Resolution::Overwrite,
        "rename" => Resolution::Rename,
        _ => Resolution::Skip,
    };
    if let Some(tx) = resolvers().lock().unwrap().get(&req_id) {
        let _ = tx.send((res, apply_all));
    }
}

/// Moves several entries to the system trash (restorable).
#[tauri::command]
pub fn trash_entries(paths: Vec<String>) -> OpResult {
    let mut ok = 0u32;
    let mut errors = Vec::new();

    for p in &paths {
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
        let path = Path::new(p);
        let res = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match res {
            Ok(_) => ok += 1,
            Err(e) => errors.push(format!("{}: {}", p, e)),
        }
    }
    OpResult { ok, errors }
}

// ============================================================
// Tests: real file-system checks of the batch rename and the
// text writing (they use a fresh temporary folder).
// ============================================================
#[cfg(test)]
mod tests {
    use super::{
        flatten_tree, hash_file_sha256, rename_batch, resolve_buf, write_text_file, CopyTask,
    };
    use std::fs;
    use std::path::PathBuf;
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

    fn write(dir: &PathBuf, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }
    fn read(dir: &PathBuf, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap()
    }
    fn exists(dir: &PathBuf, name: &str) -> bool {
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
    fn resolve_buf_clamps() {
        assert_eq!(resolve_buf(0), 256 * 1024); // Standard
        assert_eq!(resolve_buf(1), 4 * 1024); // untere Grenze
        assert_eq!(resolve_buf(512), 512 * 1024);
        assert_eq!(resolve_buf(1_000_000), 16 * 1024 * 1024); // obere Grenze
    }

    #[test]
    fn flatten_tree_collects_files_and_makes_dirs() {
        // Source tree with a subfolder; flatten must create the target folders
        // and yield one task per file (not for directories).
        let root = tmp_dir();
        let src = root.join("src");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), "A").unwrap();
        fs::write(src.join("sub/b.txt"), "B").unwrap();
        let dst = root.join("dst");

        let mut tasks: Vec<CopyTask> = Vec::new();
        flatten_tree(&src, &dst, 0, &mut tasks);

        assert_eq!(tasks.len(), 2, "zwei Dateien erwartet");
        assert!(dst.join("sub").is_dir(), "Zielordner angelegt");
        let names: Vec<String> = tasks
            .iter()
            .map(|t| t.dst.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
        assert!(tasks.iter().all(|t| t.idx == 0));
    }

    #[test]
    fn hash_file_matches_known_vector() {
        // SHA-256("abc") is a known reference value – the basis of the
        // copy verification (source vs. target).
        let d = tmp_dir();
        write(&d, "abc.bin", "abc");
        let hex: String = hash_file_sha256(&d.join("abc.bin"))
            .unwrap()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn write_text_file_respects_overwrite() {
        let d = tmp_dir();
        let p = d.join("list.txt").to_string_lossy().into_owned();
        assert_eq!(write_text_file(p.clone(), "first".into(), false), Ok(true));
        // Without overwrite: untouched -> Ok(false), the content stays.
        assert_eq!(write_text_file(p.clone(), "second".into(), false), Ok(false));
        assert_eq!(read(&d, "list.txt"), "first");
        // With overwrite: written anew.
        assert_eq!(write_text_file(p.clone(), "second".into(), true), Ok(true));
        assert_eq!(read(&d, "list.txt"), "second");
    }
}
