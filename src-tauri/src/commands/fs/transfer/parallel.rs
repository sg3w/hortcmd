use crate::vfs::StreamPath;
// ============================================================
// The parallel copy path (ticket 035).
//
// Several worker threads share one set of counters and pull file tasks
// from a common index. The tree is flattened into individual files
// first, with the target folders created up front, so no worker has to
// create a directory another might be creating at the same time.
//
// Each task remembers which top-level source it came from. That is what
// lets `commit_finished_jobs` publish only the sources whose every file
// copied cleanly and discard the rest — one failing worker never
// replaces a destination that was already there.
// ============================================================

use super::copy::{hash_file_sha256, CopyTuning};
use super::events::ItemError;
use super::plan::{discard_staging, publish, Plan};
use super::progress::SharedProg;
use crate::vfs::Provider;
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::AppHandle;

/// A single file copy task (with source index for the ok counting).
#[derive(Debug)]
pub(crate) struct CopyTask {
    pub(crate) idx: usize,
    pub(crate) src: StreamPath,
    pub(crate) dst: StreamPath,
}

/// Collect a tree without changing the destination. All jobs must finish this
/// phase before any directory is created or a worker starts writing files.
pub(crate) fn flatten_tree(
    srcp: &dyn Provider,
    directories: &mut Vec<StreamPath>,
    src: &StreamPath,
    dst: &StreamPath,
    idx: usize,
    tasks: &mut Vec<CopyTask>,
) -> std::io::Result<()> {
    if srcp.stat(src)?.is_dir {
        directories.push(dst.to_owned());
        for child in srcp.read_dir(src)? {
            let name = child.file_name().unwrap_or_default();
            flatten_tree(srcp, directories, &child, &dst.join(name)?, idx, tasks)?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            if directories.last() != Some(&parent) {
                directories.push(parent);
            }
        }
        tasks.push(CopyTask {
            idx,
            src: src.to_owned(),
            dst: dst.to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn prepare_jobs(
    srcp: &dyn Provider,
    dstp: &dyn Provider,
    jobs: &[(StreamPath, Plan)],
) -> Result<Vec<CopyTask>, Vec<ItemError>> {
    let mut tasks = Vec::new();
    let mut directories = Vec::new();
    let mut created = Vec::new();
    let result = (|| -> Result<(), Vec<ItemError>> {
        // A failure reading any source leaves every destination untouched.
        for (i, (src, plan)) in jobs.iter().enumerate() {
            let mut job_dirs = Vec::new();
            flatten_tree(srcp, &mut job_dirs, src, &plan.write_to, i, &mut tasks)
                .map_err(|error| vec![ItemError::new(src.display().to_string(), error.into())])?;
            directories.push(job_dirs);
        }
        for (i, ((src, plan), dirs)) in jobs.iter().zip(directories).enumerate() {
            plan.writing()
                .map_err(|error| vec![ItemError::new(src.display().to_string(), error.into())])?;
            for dir in dirs {
                let existed = dstp.exists(&dir);
                let result = dstp.create_dir_all(&dir);
                if !existed && dstp.exists(&dir) {
                    created.push((i, dir.clone()));
                }
                result.map_err(|error| {
                    vec![ItemError::new(dir.display().to_string(), error.into())]
                })?;
            }
        }
        Ok(())
    })();
    if let Err(mut errors) = result {
        for (_, plan) in jobs {
            discard_staging(dstp, plan, &mut errors);
        }
        // Direct output has no exclusive ownership. Never recursively delete a
        // directory another process could have populated; report exact remnants.
        for (i, dir) in created {
            if !jobs[i].1.is_staged() && dstp.exists(&dir) {
                errors.push(ItemError::new(
                    dir.display().to_string(),
                    crate::errors::AppError::new(crate::errors::code::TRANSFER_PARTIAL_DIRECTORIES)
                        .op("prepare-directories"),
                ));
            }
        }
        return Err(errors);
    }
    Ok(tasks)
}

/// Copies a file block by block with shared bookkeeping (parallel path).
pub(crate) fn parallel_copy_file(
    task: &CopyTask,
    sp: &SharedProg,
    tune: CopyTuning,
) -> std::io::Result<()> {
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

    let abort = |writer: Box<dyn Write + Send>, dst: &StreamPath| {
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
        super::recovery::fault("write");
        if let Some(h) = hasher.as_mut() {
            h.update(&buf[..n]);
        }
        file_done += n as u64;
        sp.bytes_done.fetch_add(n as u64, Ordering::Relaxed);
        sp.emit(&name, file_done, file_total, false, false);
        sp.rate_limit(n, limit);
    }

    writer.flush()?;
    super::recovery::fault("flush");
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
pub(crate) fn run_parallel_copy(
    srcp: Arc<dyn Provider>,
    dstp: Arc<dyn Provider>,
    app: AppHandle,
    id: String,
    op: String,
    jobs: Vec<(StreamPath, Plan)>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    tune: CopyTuning,
    threads: u32,
    files_total: u32,
    bytes_total: u64,
) -> (u32, Vec<ItemError>) {
    let tasks = match prepare_jobs(&*srcp, &*dstp, &jobs) {
        Ok(tasks) => tasks,
        Err(errors) => return (0, errors),
    };

    let sp = Arc::new(SharedProg {
        srcp,
        dstp,
        sink: Arc::new(app),
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
    let errors: Arc<Mutex<Vec<ItemError>>> = Arc::new(Mutex::new(Vec::new()));

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
            let correlation = crate::diagnostics::current();
            std::thread::spawn(move || {
                let _scope = correlation.map(crate::diagnostics::Scope::enter);
                loop {
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
                                .push(ItemError::new(task.src.display().to_string(), e.into()));
                        }
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
pub(crate) fn commit_finished_jobs(
    dstp: &dyn Provider,
    plans: &[&Plan],
    done: &[bool],
    cancelled: bool,
    errors: &mut Vec<ItemError>,
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
                errors.push(ItemError::new(plan.target.display().to_string(), e.into()));
                discard_staging(dstp, plan, errors);
            }
        }
    }
    ok
}
