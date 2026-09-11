// ============================================================
// File operations: copy/move/delete/create folder.
// Contains the shared transfer infrastructure (progress,
// cancel, name conflicts) that the archive module also uses.
// ============================================================

use crate::vfs::caps::{self, Capability};
use crate::vfs::{self, Provider};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
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
    /// Which transfer safety policy actually ran: `"safe"` (staged and
    /// committed) or `"direct"` (written straight to the final name).
    /// Empty for archive operations, which have no such choice. Recorded
    /// so a failure can be read against the mode that produced it
    /// (ticket 029).
    pub mode: String,
}

/// Recognizable remains of an interrupted failure-safe transfer –
/// event "fs-leftovers". The engine only *reports* them: staging and
/// backup entries may hold the only copy of the user's data, so nothing
/// is deleted without an explicit answer (ticket 029).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Leftovers {
    /// The destination folder as an address, for the message.
    pub dir: String,
    /// Full addresses of the leftover entries.
    pub paths: Vec<String>,
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
fn measure(provider: &dyn Provider, p: &Path) -> (u32, u64) {
    match provider.stat(p) {
        Ok(s) if s.is_dir => match provider.read_dir(p) {
            Ok(children) => children.iter().fold((0, 0), |(f, b), c| {
                let (cf, cb) = measure(provider, c);
                (f + cf, b + cb)
            }),
            Err(_) => (0, 0),
        },
        Ok(s) => (1, s.len),
        // Unreadable/missing entry: count as one 0-byte file (as before).
        Err(_) => (1, 0),
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
fn free_name(provider: &dyn Provider, target: &Path) -> PathBuf {
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
        if !provider.exists(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// Asks the frontend for the conflict resolution and blocks until the answer.
/// Reacts to cancel (skip) so the thread never hangs.
fn ask_collision(
    app: &AppHandle,
    provider: &dyn Provider,
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
            is_dir: provider.is_dir(target),
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

/// The three knobs every copy shares, from the settings down to the
/// innermost block loop. Carried as one value so the recursive and
/// parallel paths do not thread three unrelated parameters through
/// every level — which is what pushed several of these functions past
/// a readable signature.
#[derive(Clone, Copy)]
struct CopyTuning {
    /// Speed cap in bytes per second; 0 = unlimited.
    limit: u64,
    /// Compare the SHA-256 of source and target after copying.
    verify: bool,
    /// Block size of the copy loop.
    buf_size: usize,
}

/// Copies a file in blocks; reacts to cancel (the partial file is
/// removed) and pause.
fn copy_file_prog(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &Path,
    dst: &Path,
    prog: &mut Prog,
    tune: CopyTuning,
) -> std::io::Result<()> {
    let CopyTuning {
        limit,
        verify,
        buf_size,
    } = tune;
    use sha2::{Digest, Sha256};

    let file_total = srcp.stat(src)?.len;
    let name = src.to_string_lossy().into_owned();
    let mut reader = srcp.open_read(src)?;
    let mut writer = dstp.create_write(dst)?;
    let mut buf = vec![0u8; buf_size];
    let mut file_done = 0u64;
    let mut hasher = verify.then(Sha256::new);

    // Rate-limit window (only for limit > 0): reset roughly every 1 s.
    let mut win_start = Instant::now();
    let mut win_bytes = 0u64;

    let abort = |writer: Box<dyn Write + Send>, dst: &Path| {
        drop(writer);
        let _ = dstp.remove_file(dst); // unvollständige Zieldatei verwerfen
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
        let dst_hash = hash_file_sha256(dstp, dst)?;
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
fn hash_file_sha256(provider: &dyn Provider, path: &Path) -> std::io::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let mut f = provider.open_read(path)?;
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

// ============================================================
// Failure-safe staging and commit (ticket 029).
//
// Safe mode never writes under the final name and never destroys an
// existing destination before the replacement is complete: each
// top-level source is copied into a hidden staging entry *on the
// destination store* and published with a single rename. Direct mode
// keeps the original behaviour — the destination is removed up front
// and the bytes go straight to the final name.
// ============================================================

/// Prefix of the entry a safe transfer writes into.
const STAGE_PREFIX: &str = ".hortcmd-stage-";
/// Prefix of the old destination while a replacement is committed.
const BACKUP_PREFIX: &str = ".hortcmd-backup-";

/// Whether a name is one of hortcmd's transfer working entries. They are
/// hidden and prefixed so they are never produced as normal output and
/// never collide with a user-created name.
fn is_working_entry(name: &str) -> bool {
    name.starts_with(STAGE_PREFIX) || name.starts_with(BACKUP_PREFIX)
}

/// A free working name inside `dir`. Process id and a global counter
/// keep parallel workers — and separate app instances — apart; the
/// existence check closes the remaining gap.
fn working_name(provider: &dyn Provider, dir: &Path, prefix: &str) -> PathBuf {
    let pid = std::process::id();
    loop {
        let n = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
        let cand = dir.join(format!("{}{}-{}", prefix, pid, n));
        if !provider.exists(&cand) {
            return cand;
        }
    }
}

/// How one top-level source reaches its final name.
struct Plan {
    /// Where the bytes are written: the staging entry in safe mode, the
    /// final name in direct mode.
    write_to: PathBuf,
    /// The name the user asked for.
    target: PathBuf,
    /// The final name still exists and has to be replaced on commit.
    /// Only ever true in safe mode — direct mode deletes it up front.
    replace: bool,
}

impl Plan {
    /// Direct mode: the bytes already go to their final name.
    fn direct(target: PathBuf) -> Self {
        Plan {
            write_to: target.clone(),
            target,
            replace: false,
        }
    }

    /// Safe mode: a hidden sibling of the target receives the bytes.
    fn staged(provider: &dyn Provider, target: PathBuf, replace: bool) -> Self {
        let dir = target.parent().unwrap_or(Path::new("/")).to_path_buf();
        Plan {
            write_to: working_name(provider, &dir, STAGE_PREFIX),
            target,
            replace,
        }
    }

    fn is_staged(&self) -> bool {
        self.write_to != self.target
    }
}

/// Publishes `from` under `to`. The old destination survives until the
/// replacement is in place: either through a documented atomic replace,
/// or through a backup rename that is undone when the commit fails.
pub(crate) fn commit(
    dstp: &dyn Provider,
    from: &Path,
    to: &Path,
    replace: bool,
) -> std::io::Result<()> {
    if !replace {
        return dstp.rename(from, to);
    }
    // A rename that replaces atomically leaves no moment without a
    // destination. `rename` onto a populated directory is not portable,
    // so a directory target always takes the backup route.
    if dstp.rename_replaces() && !dstp.is_dir(to) {
        return dstp.rename(from, to);
    }
    let dir = to.parent().unwrap_or(Path::new("/")).to_path_buf();
    let backup = working_name(dstp, &dir, BACKUP_PREFIX);
    dstp.rename(to, &backup)?;
    match dstp.rename(from, to) {
        Ok(()) => {
            // Only now is the old destination redundant. A failure to
            // clear it leaves a visible leftover, never a lost file.
            let _ = dstp.remove(&backup);
            Ok(())
        }
        Err(e) => {
            let _ = dstp.rename(&backup, to);
            Err(e)
        }
    }
}

/// Picks how one source reaches its final name. Direct mode writes
/// there straight away (its collision was already resolved by deleting);
/// safe mode writes beside it and commits.
fn plan_for(dstp: &dyn Provider, target: PathBuf, replace: bool, safe: bool) -> Plan {
    if safe {
        Plan::staged(dstp, target, replace)
    } else {
        Plan::direct(target)
    }
}

/// Publishes a finished plan. A no-op in direct mode.
fn publish(dstp: &dyn Provider, plan: &Plan) -> std::io::Result<()> {
    if !plan.is_staged() {
        return Ok(());
    }
    commit(dstp, &plan.write_to, &plan.target, plan.replace)
}

/// Removes the staging entry after a failed or cancelled transfer. Only
/// the staging entry — the destination is either untouched or already
/// committed. Cleanup problems are reported on their own so they are not
/// mistaken for the failure that caused them.
fn discard_staging(dstp: &dyn Provider, plan: &Plan, errors: &mut Vec<String>) {
    if !plan.is_staged() || !dstp.exists(&plan.write_to) {
        return;
    }
    if let Err(e) = dstp.remove(&plan.write_to) {
        errors.push(format!(
            "Aufräumen von {} fehlgeschlagen: {}",
            plan.write_to.display(),
            e
        ));
    }
}

/// Working entries left in `dir` by an earlier, interrupted run. They
/// are only reported — such an entry can hold the only copy of a file,
/// so nothing here deletes anything.
fn stale_working_entries(provider: &dyn Provider, dir: &Path) -> Vec<String> {
    let Ok(children) = provider.read_dir(dir) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|c| c.file_name()?.to_str().map(str::to_string))
        .filter(|name| is_working_entry(name))
        .collect()
}

/// Copies recursively (file or directory tree) from `srcp` to `dstp`.
fn copy_tree(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &Path,
    dst: &Path,
    prog: &mut Prog,
    tune: CopyTuning,
) -> std::io::Result<()> {
    if srcp.is_dir(src) {
        dstp.create_dir_all(dst)?;
        for child in srcp.read_dir(src)? {
            let name = child.file_name().unwrap_or_default();
            copy_tree(srcp, dstp, &child, &dst.join(name), prog, tune)?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            dstp.create_dir_all(parent)?;
        }
        copy_file_prog(srcp, dstp, src, dst, prog, tune)?;
    }
    Ok(())
}

/// The order that makes safe mode safe: write the bytes, *then* commit,
/// and only then remove a move's source. `copy` produces the bytes in
/// `plan.write_to`; it is a parameter so this ordering can be tested
/// with a failure injected at each step.
fn transfer_one_with<F>(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &Path,
    plan: &Plan,
    remove_source: bool,
    errors: &mut Vec<String>,
    copy: F,
) -> std::io::Result<()>
where
    F: FnOnce() -> std::io::Result<()>,
{
    // Until this returns, the old destination has not been touched.
    if let Err(e) = copy() {
        discard_staging(dstp, plan, errors);
        return Err(e);
    }
    if let Err(e) = publish(dstp, plan) {
        discard_staging(dstp, plan, errors);
        return Err(e);
    }
    // The destination is committed. A source that cannot be removed
    // leaves two copies — which is reported, never "repaired" by
    // deleting the copy that just succeeded.
    if remove_source {
        if let Err(e) = srcp.remove(src) {
            errors.push(format!(
                "{}: Quelle konnte nach der Übertragung nicht entfernt werden ({}) — \
                 beide Kopien bleiben bestehen",
                src.display(),
                e
            ));
        }
    }
    Ok(())
}

/// Copies one top-level source and publishes it under its final name.
#[allow(clippy::too_many_arguments)]
fn transfer_one(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &Path,
    plan: &Plan,
    prog: &mut Prog,
    tune: CopyTuning,
    remove_source: bool,
    errors: &mut Vec<String>,
) -> std::io::Result<()> {
    transfer_one_with(srcp, dstp, src, plan, remove_source, errors, || {
        copy_tree(srcp, dstp, src, &plan.write_to, prog, tune)
    })
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
    srcp: Arc<dyn Provider>,
    dstp: Arc<dyn Provider>,
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
fn flatten_tree(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    src: &Path,
    dst: &Path,
    idx: usize,
    tasks: &mut Vec<CopyTask>,
) {
    if srcp.is_dir(src) {
        let _ = dstp.create_dir_all(dst);
        if let Ok(children) = srcp.read_dir(src) {
            for child in children {
                let name = child.file_name().unwrap_or_default();
                flatten_tree(srcp, dstp, &child, &dst.join(name), idx, tasks);
            }
        }
    } else {
        if let Some(parent) = dst.parent() {
            let _ = dstp.create_dir_all(parent);
        }
        tasks.push(CopyTask {
            idx,
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
        });
    }
}

/// Copies a file block by block with shared bookkeeping (parallel path).
fn parallel_copy_file(task: &CopyTask, sp: &SharedProg, tune: CopyTuning) -> std::io::Result<()> {
    use sha2::{Digest, Sha256};

    let CopyTuning {
        limit,
        verify,
        buf_size,
    } = tune;

    let srcp = &*sp.srcp;
    let dstp = &*sp.dstp;
    let file_total = srcp.stat(&task.src)?.len;
    let name = task.src.to_string_lossy().into_owned();
    let mut reader = srcp.open_read(&task.src)?;
    let mut writer = dstp.create_write(&task.dst)?;
    let mut buf = vec![0u8; buf_size];
    let mut hasher = verify.then(Sha256::new);
    let mut file_done = 0u64;

    let abort = |writer: Box<dyn Write + Send>, dst: &Path| {
        drop(writer);
        let _ = dstp.remove_file(dst);
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
        let dst_hash = hash_file_sha256(dstp, &task.dst)?;
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
    srcp: Arc<dyn Provider>,
    dstp: Arc<dyn Provider>,
    app: AppHandle,
    id: String,
    op: String,
    jobs: Vec<(PathBuf, Plan)>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    tune: CopyTuning,
    threads: u32,
    files_total: u32,
    bytes_total: u64,
) -> (u32, Vec<String>) {
    // Flatten all source trees into file tasks (target folders are
    // created). In safe mode every job writes into its own staging
    // entry, so one worker's failure can never publish a half-copied
    // tree or disturb a pre-existing destination.
    let mut tasks = Vec::new();
    for (i, (src, plan)) in jobs.iter().enumerate() {
        flatten_tree(&*srcp, &*dstp, src, &plan.write_to, i, &mut tasks);
    }

    let sp = Arc::new(SharedProg {
        srcp,
        dstp,
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
                match parallel_copy_file(task, sp.as_ref(), tune) {
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

    let mut errors = Arc::try_unwrap(errors)
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();

    // Commit phase. Only now — with every worker joined — is a job's
    // staging entry known to be complete, so publishing here is what
    // keeps a failed worker from replacing anything.
    let plans: Vec<&Plan> = jobs.iter().map(|(_, plan)| plan).collect();
    let done: Vec<bool> = (0..jobs.len())
        .map(|i| !failed[i].load(Ordering::Relaxed))
        .collect();
    let ok = commit_finished_jobs(&*sp.dstp, &plans, &done, sp.cancelled(), &mut errors);
    (ok, errors)
}

/// Publishes the jobs whose every file copied cleanly, and discards the
/// staging entries of the rest. A worker that failed — or a cancelled
/// run — therefore never replaces a destination that was already there.
/// Returns the number of published jobs.
fn commit_finished_jobs(
    dstp: &dyn Provider,
    plans: &[&Plan],
    done: &[bool],
    cancelled: bool,
    errors: &mut Vec<String>,
) -> u32 {
    let mut ok = 0u32;
    for (i, plan) in plans.iter().enumerate() {
        if cancelled || !done.get(i).copied().unwrap_or(false) {
            discard_staging(dstp, plan, errors);
            continue;
        }
        match publish(dstp, plan) {
            Ok(()) => ok += 1,
            Err(e) => {
                errors.push(format!("{}: {}", plan.target.display(), e));
                discard_staging(dstp, plan, errors);
            }
        }
    }
    ok
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
    safe: bool,
) {
    let tune = CopyTuning {
        limit,
        verify,
        buf_size: resolve_buf(buf_kb),
    };
    let mode = if safe { "safe" } else { "direct" };
    let fail = |id: String, op: String, err: String| {
        let _ = app.emit(
            "fs-done",
            OpDone {
                id,
                op,
                ok: 0,
                errors: vec![err],
                cancelled: false,
                mode: mode.to_string(),
            },
        );
    };

    // Source and destination are asked for their own capability before
    // anything is resolved (TICKET-031): an allowed source is no licence
    // for the target, and a move additionally needs to remove at the
    // source. A refusal ends the operation through the same `fs-done`
    // event as any other failure.
    let src_cap = if remove_source {
        Capability::Move
    } else {
        Capability::CopyFrom
    };
    for src in &sources {
        if let Err(e) = caps::require_pair(src, src_cap, &dest_dir, Capability::CopyTo) {
            return fail(id, op, e);
        }
    }

    // Resolve the destination and the source location through the VFS
    // registry. The engine holds a source provider and a destination
    // provider, so a transfer may cross providers (local↔remote).
    let (dstp, dest_path) = match vfs::resolve(&dest_dir) {
        Ok(v) => v,
        Err(e) => return fail(id, op, e),
    };
    let dest = PathBuf::from(&dest_path);
    // Safe mode never degrades to direct writes behind the user's back:
    // a destination that cannot publish a staged entry stops the
    // transfer with something the user can act on.
    if safe && !dstp.supports_staging() {
        return fail(
            id,
            op,
            format!(
                "Ausfallsichere Übertragung wird von {} nicht unterstützt — \
                 in den Einstellungen unter Dateioperationen abschalten, \
                 um hierher zu übertragen",
                dest_dir
            ),
        );
    }
    // All sources of one operation come from the same location; resolve
    // the source provider from the first and require the rest to match.
    let srcp = match sources.first() {
        Some(first) => match vfs::resolve(first) {
            Ok((p, _)) => p,
            Err(e) => return fail(id, op, e),
        },
        None => return fail(id, op, "Keine Quellen angegeben".into()),
    };
    // The in-place `rename` fast-path only applies within one store.
    let same_store = srcp.store_id() == dstp.store_id();

    // Parallel path only for pure copying within one store and thread
    // count > 1. Cross-store copies stay sequential (streamed).
    let parallel = !remove_source && threads > 1 && same_store;

    // Resolve + measure each source up front (totals + bookkeeping for a
    // rename move). `measured` holds the resolved provider-local path.
    let mut errors: Vec<String> = Vec::new();
    let measured: Vec<(String, u32, u64)> = sources
        .iter()
        .filter_map(|s| match vfs::resolve(s) {
            Ok((sp, spath)) if sp.store_id() == srcp.store_id() => {
                let (f, b) = measure(&*srcp, Path::new(&spath));
                Some((spath, f, b))
            }
            Ok(_) => {
                errors.push(format!("{}: Quelle aus einem anderen Speicher", s));
                None
            }
            Err(e) => {
                errors.push(format!("{}: {}", s, e));
                None
            }
        })
        .collect();
    let files_total = measured.iter().map(|m| m.1).sum();
    let bytes_total: u64 = measured.iter().map(|m| m.2).sum();

    // Staging needs room for the copy *next to* the destination. Only a
    // definite shortage stops the transfer — an unknown capacity lets
    // the write itself decide, and either way the old destination is
    // still untouched at this point.
    if safe {
        if let Some(free) = dstp.free_space(&dest) {
            if free < bytes_total {
                return fail(
                    id,
                    op,
                    format!(
                        "Zu wenig Platz für die ausfallsichere Zwischenablage: \
                         {} Bytes nötig, {} Bytes frei",
                        bytes_total, free
                    ),
                );
            }
        }
        // Remains of an earlier, interrupted run. Reported, never
        // deleted — such an entry can hold the only copy of a file.
        let stale = stale_working_entries(&*dstp, &dest);
        if !stale.is_empty() {
            let base = dest_dir.trim_end_matches('/');
            let _ = app.emit(
                "fs-leftovers",
                Leftovers {
                    dir: dest_dir.clone(),
                    paths: stale.iter().map(|n| format!("{}/{}", base, n)).collect(),
                },
            );
        }
    }

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
    let mut default_res: Option<Resolution> = None; // „Für alle übernehmen"
                                                    // In parallel mode the resolved pairs are collected and afterwards
                                                    // copied in parallel (collisions are resolved here sequentially/interactively).
    let mut copy_jobs: Vec<(PathBuf, Plan)> = Vec::new();

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
        // `replace` records an accepted overwrite: in safe mode the old
        // destination stays put until the commit, in direct mode it is
        // removed here as before.
        let mut replace = false;
        if dstp.exists(&target) {
            let res = match default_res {
                Some(r) => r,
                None => {
                    let (r, apply_all) = ask_collision(&app, &*dstp, &id, &target, &cancel);
                    if apply_all {
                        default_res = Some(r);
                    }
                    r
                }
            };
            match res {
                Resolution::Skip => continue,
                Resolution::Rename => target = free_name(&*dstp, &target),
                Resolution::Overwrite if safe => replace = true,
                Resolution::Overwrite => {
                    if let Err(e) = dstp.remove(&target) {
                        errors.push(format!("{}: {}", s, e));
                        continue;
                    }
                }
            }
        }

        let plan = plan_for(&*dstp, target, replace, safe);

        // Parallel mode (copy only): remember the plan, actual copying happens later.
        if parallel {
            copy_jobs.push((src.to_path_buf(), plan));
            continue;
        }

        let result = if remove_source
            && same_store
            && commit(&*dstp, src, &plan.target, plan.replace).is_ok()
        {
            // Move inside one store: the rename *is* the commit, so the
            // fast path needs no staging. A replacement still goes
            // through the same backup dance, so the old destination
            // survives a failed rename.
            prog.files_done += files;
            prog.bytes_done += bytes;
            prog.emit(&src.to_string_lossy(), *bytes, *bytes, true);
            Ok(())
        } else {
            transfer_one(
                &*srcp,
                &*dstp,
                src,
                &plan,
                &mut prog,
                tune,
                remove_source,
                &mut errors,
            )
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
            srcp.clone(),
            dstp.clone(),
            app.clone(),
            id.clone(),
            op.clone(),
            copy_jobs,
            cancel.clone(),
            pause.clone(),
            tune,
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
            mode: mode.to_string(),
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

/// Finds a free temporary name in the same folder (hidden file).
fn unique_temp(provider: &dyn Provider, base: &Path) -> PathBuf {
    loop {
        let n = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
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

// ============================================================
// Tests: real file-system checks of the batch rename and the
// text writing (they use a fresh temporary folder).
// ============================================================
#[cfg(test)]
mod tests {
    use super::{
        flatten_tree, hash_file_sha256, rename_batch, resolve_buf, write_text_file, CopyTask,
    };
    use crate::vfs::local::LocalProvider;
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
        flatten_tree(&LocalProvider, &LocalProvider, &src, &dst, 0, &mut tasks);

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
        let hex: String = hash_file_sha256(&LocalProvider, &d.join("abc.bin"))
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

// ============================================================
// Failure-safe transfers (ticket 029).
//
// The guarantee under test is an ordering one: bytes reach a staging
// entry first, the final name appears only on commit, and a move's
// source goes last. So the copy step is injected — every test drives a
// different failure through it and then asserts what survived.
// ============================================================
#[cfg(test)]
mod safe_tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
    use std::sync::{Arc, Mutex};

    // ---------- An in-memory provider with injectable failures ----------

    #[derive(Default)]
    struct FakeFs {
        files: HashMap<PathBuf, Vec<u8>>,
        dirs: HashSet<PathBuf>,
    }

    struct Fake {
        fs: Arc<Mutex<FakeFs>>,
        /// `rename` fails when the source is in here.
        fail_rename: Mutex<HashSet<PathBuf>>,
        /// `remove`/`remove_file` fails when the path is in here.
        fail_remove: Mutex<HashSet<PathBuf>>,
        replaces: bool,
    }

    impl Fake {
        fn new(replaces: bool) -> Arc<Fake> {
            Arc::new(Fake {
                fs: Arc::new(Mutex::new(FakeFs::default())),
                fail_rename: Mutex::new(HashSet::new()),
                fail_remove: Mutex::new(HashSet::new()),
                replaces,
            })
        }
        fn put(&self, path: &str, data: &str) {
            self.fs
                .lock()
                .unwrap()
                .files
                .insert(PathBuf::from(path), data.as_bytes().to_vec());
        }
        fn get(&self, path: &str) -> Option<String> {
            self.fs
                .lock()
                .unwrap()
                .files
                .get(Path::new(path))
                .map(|b| String::from_utf8_lossy(b).into_owned())
        }
        fn names_in(&self, dir: &str) -> Vec<String> {
            let fs = self.fs.lock().unwrap();
            fs.files
                .keys()
                .chain(fs.dirs.iter())
                .filter(|p| p.parent() == Some(Path::new(dir)))
                .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
                .collect()
        }
    }

    struct FakeWriter {
        fs: Arc<Mutex<FakeFs>>,
        path: PathBuf,
        buf: Vec<u8>,
    }

    impl Write for FakeWriter {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.fs
                .lock()
                .unwrap()
                .files
                .insert(self.path.clone(), self.buf.clone());
            Ok(())
        }
    }

    impl Drop for FakeWriter {
        fn drop(&mut self) {
            let _ = self.flush();
        }
    }

    impl Provider for Fake {
        fn store_id(&self) -> String {
            "fake".into()
        }
        fn list(&self, _path: &str) -> Result<crate::commands::fs::dir::DirListing, String> {
            Err("nicht unterstützt".into())
        }
        fn stat(&self, path: &Path) -> io::Result<crate::vfs::Stat> {
            let fs = self.fs.lock().unwrap();
            if fs.dirs.contains(path) {
                return Ok(crate::vfs::Stat {
                    is_dir: true,
                    len: 0,
                });
            }
            fs.files
                .get(path)
                .map(|b| crate::vfs::Stat {
                    is_dir: false,
                    len: b.len() as u64,
                })
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "nicht gefunden"))
        }
        fn exists(&self, path: &Path) -> bool {
            let fs = self.fs.lock().unwrap();
            fs.files.contains_key(path) || fs.dirs.contains(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.fs.lock().unwrap().dirs.contains(path)
        }
        fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
            let fs = self.fs.lock().unwrap();
            Ok(fs
                .files
                .keys()
                .chain(fs.dirs.iter())
                .filter(|p| p.parent() == Some(path))
                .cloned()
                .collect())
        }
        fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
            let fs = self.fs.lock().unwrap();
            let data = fs
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "nicht gefunden"))?;
            Ok(Box::new(std::io::Cursor::new(data)))
        }
        fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
            Ok(Box::new(FakeWriter {
                fs: Arc::clone(&self.fs),
                path: path.to_path_buf(),
                buf: Vec::new(),
            }))
        }
        fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
            self.fs
                .lock()
                .unwrap()
                .files
                .insert(path.to_path_buf(), data.to_vec());
            Ok(())
        }
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            self.fs.lock().unwrap().dirs.insert(path.to_path_buf());
            Ok(())
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.fail_rename.lock().unwrap().contains(from) {
                return Err(io::Error::other("rename abgelehnt (Test)"));
            }
            let mut fs = self.fs.lock().unwrap();
            if let Some(data) = fs.files.remove(from) {
                fs.files.insert(to.to_path_buf(), data);
                return Ok(());
            }
            if fs.dirs.remove(from) {
                fs.dirs.insert(to.to_path_buf());
                return Ok(());
            }
            Err(io::Error::new(ErrorKind::NotFound, "nicht gefunden"))
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            if self.fail_remove.lock().unwrap().contains(path) {
                return Err(io::Error::other("remove abgelehnt (Test)"));
            }
            self.fs.lock().unwrap().files.remove(path);
            Ok(())
        }
        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            if self.fail_remove.lock().unwrap().contains(path) {
                return Err(io::Error::other("remove abgelehnt (Test)"));
            }
            self.fs.lock().unwrap().dirs.remove(path);
            Ok(())
        }
        fn supports_staging(&self) -> bool {
            true
        }
        fn rename_replaces(&self) -> bool {
            self.replaces
        }
    }

    // ---------- Helpers ----------

    static N: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rc-safe-{}-{}",
            std::process::id(),
            N.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fail() -> io::Error {
        io::Error::other("Kopieren fehlgeschlagen (Test)")
    }

    // ---------- Plan / naming ----------

    #[test]
    fn direct_mode_writes_straight_to_the_target() {
        let plan = Plan::direct(PathBuf::from("/dst/report.txt"));
        assert!(!plan.is_staged());
        assert_eq!(plan.write_to, plan.target);
        assert!(!plan.replace);
    }

    #[test]
    fn staged_mode_writes_next_to_the_target() {
        let fake = Fake::new(false);
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/report.txt"), true);
        assert!(plan.is_staged());
        assert_eq!(plan.write_to.parent(), plan.target.parent());
        let name = plan.write_to.file_name().unwrap().to_str().unwrap();
        assert!(is_working_entry(name), "not recognizable: {}", name);
        assert!(name.starts_with('.'), "staging entries must be hidden");
    }

    #[test]
    fn working_names_are_unique_and_avoid_existing_entries() {
        let fake = Fake::new(false);
        let a = working_name(&*fake, Path::new("/dst"), STAGE_PREFIX);
        fake.put(a.to_str().unwrap(), "taken");
        let b = working_name(&*fake, Path::new("/dst"), STAGE_PREFIX);
        assert_ne!(a, b);
        assert!(!fake.exists(&b));
    }

    #[test]
    fn only_our_own_prefixes_count_as_working_entries() {
        assert!(is_working_entry(".hortcmd-stage-1-2"));
        assert!(is_working_entry(".hortcmd-backup-1-2"));
        assert!(!is_working_entry("report.txt"));
        assert!(!is_working_entry(".hidden"));
        assert!(!is_working_entry(".hortcmd-notes"));
    }

    #[test]
    fn leftovers_are_reported_but_never_touched() {
        let fake = Fake::new(false);
        fake.put("/dst/.hortcmd-stage-9-1", "half a copy");
        fake.put("/dst/keep.txt", "user data");
        let stale = stale_working_entries(&*fake, Path::new("/dst"));
        assert_eq!(stale, vec![".hortcmd-stage-9-1".to_string()]);
        // Reporting is all it does.
        assert_eq!(
            fake.get("/dst/.hortcmd-stage-9-1").as_deref(),
            Some("half a copy")
        );
    }

    // ---------- Commit ----------

    #[test]
    fn a_new_destination_is_committed_with_one_rename() {
        let fake = Fake::new(false);
        fake.put("/dst/.stage", "new");
        commit(
            &*fake,
            Path::new("/dst/.stage"),
            Path::new("/dst/f.txt"),
            false,
        )
        .unwrap();
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
        assert!(fake.get("/dst/.stage").is_none());
    }

    #[test]
    fn an_atomic_replace_is_used_where_the_provider_has_one() {
        let fake = Fake::new(true);
        fake.put("/dst/f.txt", "old");
        fake.put("/dst/.stage", "new");
        commit(
            &*fake,
            Path::new("/dst/.stage"),
            Path::new("/dst/f.txt"),
            true,
        )
        .unwrap();
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
        // No backup was needed, so none is left behind.
        assert!(fake
            .names_in("/dst")
            .iter()
            .all(|n| !n.starts_with(BACKUP_PREFIX)));
    }

    #[test]
    fn without_an_atomic_replace_the_old_destination_is_kept_until_the_end() {
        let fake = Fake::new(false);
        fake.put("/dst/f.txt", "old");
        fake.put("/dst/.stage", "new");
        commit(
            &*fake,
            Path::new("/dst/.stage"),
            Path::new("/dst/f.txt"),
            true,
        )
        .unwrap();
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
        // The backup is cleared only after the new name is in place.
        assert_eq!(fake.names_in("/dst"), vec!["f.txt".to_string()]);
    }

    #[test]
    fn a_failed_commit_puts_the_old_destination_back() {
        let fake = Fake::new(false);
        fake.put("/dst/f.txt", "old");
        fake.put("/dst/.stage", "new");
        fake.fail_rename
            .lock()
            .unwrap()
            .insert(PathBuf::from("/dst/.stage"));

        let err = commit(
            &*fake,
            Path::new("/dst/.stage"),
            Path::new("/dst/f.txt"),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("abgelehnt"));
        // Byte-for-byte the same file as before, under its own name.
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
        assert_eq!(fake.get("/dst/.stage").as_deref(), Some("new"));
        assert_eq!(
            fake.names_in("/dst").len(),
            2,
            "the rollback must leave no backup behind: {:?}",
            fake.names_in("/dst")
        );
    }

    // ---------- Ordering: copy, commit, source removal ----------

    #[test]
    fn a_failed_copy_leaves_the_old_destination_untouched() {
        let fake = Fake::new(false);
        fake.put("/dst/f.txt", "old");
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), true);
        fake.put(plan.write_to.to_str().unwrap(), "half");

        let mut errors = Vec::new();
        let err = transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            false,
            &mut errors,
            || Err(fail()),
        )
        .unwrap_err();

        assert!(err.to_string().contains("Test"));
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
        assert!(!fake.exists(&plan.write_to), "staging entry must be gone");
        assert!(errors.is_empty(), "clean cleanup reports nothing");
    }

    #[test]
    fn a_cancelled_new_copy_leaves_no_file_under_the_final_name() {
        let fake = Fake::new(false);
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/new.txt"), false);
        fake.put(plan.write_to.to_str().unwrap(), "partial");

        let mut errors = Vec::new();
        let _ = transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/new.txt"),
            &plan,
            false,
            &mut errors,
            || Err(io::Error::new(ErrorKind::Interrupted, "abgebrochen")),
        );
        assert!(!fake.exists(Path::new("/dst/new.txt")));
        assert!(
            fake.names_in("/dst").is_empty(),
            "{:?}",
            fake.names_in("/dst")
        );
    }

    #[test]
    fn the_final_name_appears_only_after_the_copy_is_complete() {
        let fake = Fake::new(false);
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/new.txt"), false);

        let mut errors = Vec::new();
        transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/new.txt"),
            &plan,
            false,
            &mut errors,
            || {
                // Mid-copy: the bytes exist, the final name does not.
                fake.put(plan.write_to.to_str().unwrap(), "all of it");
                assert!(!fake.exists(Path::new("/dst/new.txt")));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(fake.get("/dst/new.txt").as_deref(), Some("all of it"));
        assert!(errors.is_empty());
    }

    #[test]
    fn a_failed_commit_reports_and_keeps_the_old_destination() {
        let fake = Fake::new(false);
        fake.put("/dst/f.txt", "old");
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), true);
        fake.fail_rename
            .lock()
            .unwrap()
            .insert(plan.write_to.clone());

        let mut errors = Vec::new();
        let err = transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            false,
            &mut errors,
            || {
                fake.put(plan.write_to.to_str().unwrap(), "new");
                Ok(())
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("abgelehnt"));
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
        assert!(!fake.exists(&plan.write_to));
    }

    #[test]
    fn a_move_removes_its_source_only_after_the_commit() {
        let fake = Fake::new(false);
        fake.put("/src/f.txt", "payload");
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

        let mut errors = Vec::new();
        transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            true,
            &mut errors,
            || {
                fake.put(plan.write_to.to_str().unwrap(), "payload");
                // Still the only committed copy at this point.
                assert!(fake.exists(Path::new("/src/f.txt")));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("payload"));
        assert!(!fake.exists(Path::new("/src/f.txt")));
    }

    #[test]
    fn a_failed_move_keeps_its_source() {
        let fake = Fake::new(false);
        fake.put("/src/f.txt", "payload");
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

        let mut errors = Vec::new();
        let _ = transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            true,
            &mut errors,
            || Err(fail()),
        );
        assert_eq!(fake.get("/src/f.txt").as_deref(), Some("payload"));
    }

    #[test]
    fn an_undeletable_move_source_is_reported_and_both_copies_stay() {
        let fake = Fake::new(false);
        fake.put("/src/f.txt", "payload");
        fake.fail_remove
            .lock()
            .unwrap()
            .insert(PathBuf::from("/src/f.txt"));
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

        let mut errors = Vec::new();
        transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            true,
            &mut errors,
            || {
                fake.put(plan.write_to.to_str().unwrap(), "payload");
                Ok(())
            },
        )
        .unwrap();

        // The committed destination is never rolled back to "repair" this.
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("payload"));
        assert_eq!(fake.get("/src/f.txt").as_deref(), Some("payload"));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("beide Kopien"), "{}", errors[0]);
    }

    #[test]
    fn a_failed_cleanup_is_reported_on_its_own() {
        let fake = Fake::new(false);
        let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);
        fake.put(plan.write_to.to_str().unwrap(), "half");
        fake.fail_remove
            .lock()
            .unwrap()
            .insert(plan.write_to.clone());

        let mut errors = Vec::new();
        let err = transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            false,
            &mut errors,
            || Err(fail()),
        )
        .unwrap_err();

        assert!(err.to_string().contains("Kopieren"), "{}", err);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Aufräumen"), "{}", errors[0]);
    }

    #[test]
    fn direct_mode_keeps_writing_to_the_final_name() {
        let fake = Fake::new(false);
        let plan = Plan::direct(PathBuf::from("/dst/f.txt"));

        let mut errors = Vec::new();
        transfer_one_with(
            &*fake,
            &*fake,
            Path::new("/src/f.txt"),
            &plan,
            false,
            &mut errors,
            || {
                fake.put("/dst/f.txt", "written directly");
                Ok(())
            },
        )
        .unwrap();

        // No staging entry is created and none has to be cleaned up.
        assert_eq!(fake.names_in("/dst"), vec!["f.txt".to_string()]);
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("written directly"));
    }

    // ---------- Plan selection ----------

    #[test]
    fn safe_mode_stages_an_overwrite_instead_of_deleting_first() {
        let fake = Fake::new(false);
        fake.put("/dst/f.txt", "old");
        let plan = plan_for(&*fake, PathBuf::from("/dst/f.txt"), true, true);
        assert!(plan.is_staged());
        assert!(plan.replace);
        // Choosing the plan must not have touched anything yet.
        assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
    }

    #[test]
    fn direct_mode_never_stages() {
        let fake = Fake::new(false);
        let plan = plan_for(&*fake, PathBuf::from("/dst/f.txt"), false, false);
        assert!(!plan.is_staged());
        assert!(!plan.replace);
    }

    // ---------- Parallel commit phase ----------

    /// Builds `n` staged jobs whose bytes are already in place, as the
    /// worker pool would leave them.
    fn staged_jobs(fake: &Fake, names: &[&str]) -> Vec<Plan> {
        names
            .iter()
            .map(|name| {
                let plan = Plan::staged(
                    fake,
                    PathBuf::from(format!("/dst/{}", name)),
                    fake.exists(Path::new(&format!("/dst/{}", name))),
                );
                fake.put(plan.write_to.to_str().unwrap(), &format!("new {}", name));
                plan
            })
            .collect()
    }

    #[test]
    fn one_failed_worker_does_not_stop_the_others_from_committing() {
        let fake = Fake::new(false);
        fake.put("/dst/b.txt", "old b");
        let plans = staged_jobs(&fake, &["a.txt", "b.txt", "c.txt"]);
        let refs: Vec<&Plan> = plans.iter().collect();

        let mut errors = Vec::new();
        // The middle worker failed.
        let ok = commit_finished_jobs(&*fake, &refs, &[true, false, true], false, &mut errors);

        assert_eq!(ok, 2);
        assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("new a.txt"));
        assert_eq!(fake.get("/dst/c.txt").as_deref(), Some("new c.txt"));
        // The failed job neither published nor disturbed what was there.
        assert_eq!(fake.get("/dst/b.txt").as_deref(), Some("old b"));
        assert!(!fake.exists(&plans[1].write_to));
        assert!(errors.is_empty());
    }

    #[test]
    fn a_cancelled_parallel_run_commits_nothing() {
        let fake = Fake::new(false);
        fake.put("/dst/a.txt", "old a");
        let plans = staged_jobs(&fake, &["a.txt", "b.txt"]);
        let refs: Vec<&Plan> = plans.iter().collect();

        let mut errors = Vec::new();
        let ok = commit_finished_jobs(&*fake, &refs, &[true, true], true, &mut errors);

        assert_eq!(ok, 0);
        assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("old a"));
        assert!(!fake.exists(Path::new("/dst/b.txt")));
        assert_eq!(fake.names_in("/dst"), vec!["a.txt".to_string()]);
    }

    #[test]
    fn every_parallel_job_gets_its_own_staging_entry() {
        let fake = Fake::new(false);
        let plans = staged_jobs(&fake, &["a.txt", "b.txt", "c.txt"]);
        let mut names: Vec<_> = plans.iter().map(|p| p.write_to.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 3, "workers must not share a staging name");
    }

    // ---------- The local provider's own commit behaviour ----------

    #[test]
    fn the_local_provider_commits_and_replaces() {
        let dir = fresh_dir();
        let local = crate::vfs::local::LocalProvider;
        assert!(local.supports_staging());

        let target = dir.join("report.txt");
        std::fs::write(&target, "old").unwrap();
        let stage = dir.join(".hortcmd-stage-test");
        std::fs::write(&stage, "new").unwrap();

        commit(&local, &stage, &target, true).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(!stage.exists());
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec!["report.txt".to_string()]);
    }

    #[test]
    fn the_local_provider_commits_a_directory_over_an_existing_one() {
        let dir = fresh_dir();
        let local = crate::vfs::local::LocalProvider;

        let target = dir.join("tree");
        std::fs::create_dir_all(target.join("sub")).unwrap();
        std::fs::write(target.join("sub/old.txt"), "old").unwrap();

        let stage = dir.join(".hortcmd-stage-dir");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("new.txt"), "new").unwrap();

        // A populated directory target cannot use a plain rename — the
        // backup route has to carry it.
        commit(&local, &stage, &target, true).unwrap();
        assert!(target.join("new.txt").exists());
        assert!(!target.join("sub").exists());
        assert!(!stage.exists());
    }
}
