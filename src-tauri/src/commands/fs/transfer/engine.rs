// ============================================================
// The transfer engine (ticket 035).
//
// One function, because it is one decision sequence: resolve both
// providers, measure the totals, resolve every name conflict *before*
// copying anything, then hand the work to the sequential or the
// parallel path and report what happened.
//
// Collisions are settled up front on purpose. Asking mid-transfer would
// mean a dialog appearing minutes into a long copy; settling first also
// means the parallel path starts with a list of tasks that need no
// further questions.
// ============================================================

use super::copy::{resolve_buf, transfer_one, CopyTuning};
use super::events::{ItemError, Leftovers, OpDone};
use super::parallel::run_parallel_copy;
use super::plan::{commit, free_name, measure, plan_for, stale_working_entries, target_in, Plan};
use super::progress::Prog;
use super::registry::{ask_collision, Resolution};
use crate::errors::{code, AppError, ErrorAction};
use crate::vfs;
use crate::vfs::caps::{self, Capability};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

/// Runs a copy/move operation (runs in its own thread).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_transfer(
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
    let fail = |id: String, op: String, item: &str, err: AppError| {
        let _ = app.emit(
            "fs-done",
            OpDone {
                id,
                op,
                ok: 0,
                errors: vec![ItemError::new(item, err)],
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
            return fail(id, op, src, e);
        }
    }

    // Resolve the destination and the source location through the VFS
    // registry. The engine holds a source provider and a destination
    // provider, so a transfer may cross providers (local↔remote).
    let (dstp, dest_path) = match vfs::resolve(&dest_dir) {
        Ok(v) => v,
        Err(e) => return fail(id, op, &dest_dir, e),
    };
    let dest = PathBuf::from(&dest_path);
    // Safe mode never degrades to direct writes behind the user's back:
    // a destination that cannot publish a staged entry stops the
    // transfer with something the user can act on.
    if safe && !dstp.supports_staging() {
        return fail(
            id,
            op,
            &dest_dir,
            AppError::new(code::TRANSFER_NO_STAGING)
                .with("dest", &dest_dir)
                .provider(&dstp.store_id())
                .action(ErrorAction::Unsupported),
        );
    }
    // All sources of one operation come from the same location; resolve
    // the source provider from the first and require the rest to match.
    let srcp = match sources.first() {
        Some(first) => match vfs::resolve(first) {
            Ok((p, _)) => p,
            Err(e) => return fail(id, op, first, e),
        },
        None => {
            return fail(
                id,
                op,
                "",
                AppError::new(code::VALIDATION_BAD_INPUT).action(ErrorAction::Report),
            )
        }
    };
    // The in-place `rename` fast-path only applies within one store.
    let same_store = srcp.store_id() == dstp.store_id();

    // Parallel path only for pure copying within one store and thread
    // count > 1. Cross-store copies stay sequential (streamed).
    let parallel = !remove_source && threads > 1 && same_store;

    // Resolve + measure each source up front (totals + bookkeeping for a
    // rename move). `measured` holds the resolved provider-local path.
    let mut errors: Vec<ItemError> = Vec::new();
    let measured: Vec<(String, u32, u64)> = sources
        .iter()
        .filter_map(|s| match vfs::resolve(s) {
            Ok((sp, spath)) if sp.store_id() == srcp.store_id() => {
                let (f, b) = measure(&*srcp, Path::new(&spath));
                Some((spath, f, b))
            }
            Ok(_) => {
                errors.push(ItemError::new(
                    s,
                    AppError::new(code::TRANSFER_SOURCE_GONE).action(ErrorAction::Report),
                ));
                None
            }
            Err(e) => {
                errors.push(ItemError::new(s, e));
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
                    &dest_dir,
                    AppError::new(code::FS_NO_SPACE)
                        .with("needed", bytes_total)
                        .with("free", free)
                        .action(ErrorAction::FreeSpace),
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
        sink: Arc::new(app.clone()),
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
            errors.push(ItemError::new(
                s,
                AppError::new(code::FS_INVALID_NAME).action(ErrorAction::Report),
            ));
            continue;
        };
        if target == *src {
            errors.push(ItemError::new(
                s,
                AppError::new(code::TRANSFER_INTO_ITSELF).action(ErrorAction::Report),
            ));
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
                        errors.push(ItemError::new(s, e.into()));
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
            Err(e) => errors.push(ItemError::new(s, e.into())),
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
