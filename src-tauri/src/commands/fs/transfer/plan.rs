use crate::vfs::StreamPath;
// ============================================================
// Planning: where a source lands, and how it gets its final name
// (ticket 035).
//
// A transfer's destination name is decided before a single byte moves.
// Direct mode writes straight to the final name; safe mode (ticket 029)
// writes into a hidden working entry beside it and commits with one
// rename, so an interrupted transfer never leaves a half-written file
// under the name the user will look for.
//
// The rule the whole module exists to keep: **the old destination
// survives until the replacement is complete.** Everything here —
// the working-name prefixes, the backup rename, the discard path, the
// leftovers report — follows from that.
// ============================================================

use super::events::ItemError;
use super::recovery::types::{Item, Phase};
use super::recovery::{self, Handle, Step};
use crate::vfs::Provider;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Builds the target path in the target folder from the source base name.
pub(crate) fn target_in(dest_dir: &StreamPath, src: &StreamPath) -> Option<StreamPath> {
    src.file_name().and_then(|name| dest_dir.join(name).ok())
}

/// Finds a free name "Name (2)", "Name (3)" … in the target folder.
pub(crate) fn free_name(
    provider: &dyn Provider,
    target: &StreamPath,
) -> std::io::Result<StreamPath> {
    let parent = target.parent().unwrap_or_default();
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
        let candidate = parent.join(format!("{} ({}){}", stem, i, ext))?;
        if !provider.exists(&candidate) {
            return Ok(candidate);
        }
        i += 1;
    }
}

/// Prefix of the entry a safe transfer writes into.
pub(crate) const STAGE_PREFIX: &str = ".hortcmd-stage-";

/// Prefix of the old destination while a replacement is committed.
pub(crate) const BACKUP_PREFIX: &str = ".hortcmd-backup-";

/// Whether a name is one of hortcmd's transfer working entries. They are
/// hidden and prefixed so they are never produced as normal output and
/// never collide with a user-created name.
pub(crate) fn is_working_entry(name: &str) -> bool {
    name.starts_with(STAGE_PREFIX) || name.starts_with(BACKUP_PREFIX)
}

/// A free working name inside `dir`. Process id and a global counter
/// keep parallel workers — and separate app instances — apart; the
/// existence check closes the remaining gap.
pub(crate) fn working_name(
    provider: &dyn Provider,
    dir: &StreamPath,
    prefix: &str,
) -> std::io::Result<StreamPath> {
    /// Distinguishes working entries created in the same millisecond by
    /// the same process; the existence check below closes the rest of
    /// the gap. Its own counter — sharing the collision-request counter
    /// was incidental, and coupling two unrelated things through one
    /// global is exactly what this split is for.
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let pid = std::process::id();
    loop {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let cand = dir.join(format!("{}{}-{}", prefix, pid, n))?;
        if !provider.exists(&cand) {
            return Ok(cand);
        }
    }
}

/// How one top-level source reaches its final name.
pub(crate) struct Plan {
    pub(crate) preflight: Option<Arc<super::preflight::types::OperationPlan>>,
    target_stamp: Option<String>,
    pub(crate) cancel: Arc<AtomicBool>,
    /// Where the bytes are written: the staging entry in safe mode, the
    /// final name in direct mode.
    pub(crate) write_to: StreamPath,
    /// The name the user asked for.
    pub(crate) target: StreamPath,
    /// The final name still exists and has to be replaced on commit.
    /// Only ever true in safe mode — direct mode deletes it up front.
    pub(crate) replace: bool,
    pub(crate) recovery: Option<Step>,
    pub(crate) backup: Option<StreamPath>,
    pub(crate) native_move: bool,
    pub(crate) owned_root: Option<StreamPath>,
    undo: std::sync::Mutex<Option<crate::commands::fs::undo::Pending>>,
}

impl Plan {
    /// Direct mode: the bytes already go to their final name.
    pub(crate) fn direct(target: StreamPath) -> Self {
        Plan {
            preflight: None,
            target_stamp: None,
            cancel: Arc::new(AtomicBool::new(false)),
            write_to: target.clone(),
            target,
            replace: false,
            recovery: None,
            backup: None,
            native_move: false,
            owned_root: None,
            undo: std::sync::Mutex::new(None),
        }
    }

    /// Safe mode: a hidden sibling of the target receives the bytes.
    pub(crate) fn staged(
        provider: &dyn Provider,
        target: StreamPath,
        replace: bool,
    ) -> std::io::Result<Self> {
        let dir = target
            .parent()
            .unwrap_or_else(|| target.clone())
            .to_path_buf();
        Ok(Plan {
            preflight: None,
            target_stamp: None,
            cancel: Arc::new(AtomicBool::new(false)),
            write_to: working_name(provider, &dir, STAGE_PREFIX)?,
            target,
            replace,
            recovery: None,
            backup: None,
            native_move: false,
            owned_root: None,
            undo: std::sync::Mutex::new(None),
        })
    }

    pub(crate) fn observe_target(&mut self, provider: &dyn Provider) -> std::io::Result<()> {
        self.target_stamp = super::preflight::target_stamp(provider, &self.target)?;
        Ok(())
    }

    pub(crate) fn track(
        &mut self,
        handle: &Handle,
        source: &StreamPath,
        dstp: &dyn Provider,
    ) -> crate::errors::AppResult<()> {
        let intent = handle.intent()?;
        if dstp.store_id() == "local" && !self.replace {
            let kind = if self.native_move {
                "move"
            } else if matches!(intent.kind, super::recovery::types::RecoveryKind::Copy) {
                "copy"
            } else {
                "unsupported"
            };
            self.undo = std::sync::Mutex::new(crate::commands::fs::undo::prepare_group(
                kind,
                &source.to_string_lossy(),
                &self.target.to_string_lossy(),
                handle.id(),
                false,
            )?);
        }
        if self.native_move {
            if self.replace && dstp.store_id() == "local" {
                self.owned_root = Some(working_name(
                    dstp,
                    &self.target.parent().unwrap_or_else(|| self.target.clone()),
                    STAGE_PREFIX,
                )?);
            }
            self.write_to = source.to_path_buf();
        }
        if self.is_staged() && !self.native_move && dstp.store_id() == "local" {
            self.owned_root = Some(self.write_to.clone());
            self.write_to = self.write_to.join("data")?;
        }
        let parent = self
            .target
            .parent()
            .unwrap_or_else(|| StreamPath::from("/"));
        self.backup = self
            .replace
            .then(|| {
                self.owned_root
                    .as_ref()
                    .map(|p| p.join("backup"))
                    .unwrap_or_else(|| working_name(dstp, &parent, BACKUP_PREFIX))
            })
            .transpose()?;
        self.recovery = Some(
            handle.track(Item {
                source: Some(intent.sources[0].with_path(source)?),
                target: intent.destination.with_path(&self.target)?,
                staging: if self.is_staged() && !self.native_move {
                    Some(
                        intent
                            .destination
                            .with_path(self.owned_root.as_ref().unwrap_or(&self.write_to))?,
                    )
                } else {
                    None
                },
                backup: self
                    .backup
                    .as_ref()
                    .map(|p| intent.destination.with_path(p))
                    .transpose()?,
                phase: Phase::Planned,
                digest: None,
                durable: false,
                owner: None,
            })?,
        );
        if let Some(root) = &self.owned_root {
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(root.native()?)?;
            recovery::durability::sync_parent(root.native()?)?;
            if let Some(step) = &self.recovery {
                step.own(intent.destination.with_path(root)?)?;
            }
            recovery::fault("staging");
        }
        Ok(())
    }

    pub(crate) fn writing(&self) -> std::io::Result<()> {
        if let Some(step) = &self.recovery {
            step.transition(Phase::Writing)
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    }

    pub(crate) fn is_staged(&self) -> bool {
        self.write_to != self.target
    }
}

/// Publishes `from` under `to`. The old destination survives until the
/// replacement is in place: either through a documented atomic replace,
/// or through a backup rename that is undone when the commit fails.
pub(crate) fn commit(
    dstp: &dyn Provider,
    from: &StreamPath,
    to: &StreamPath,
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
    let dir = to
        .parent()
        .unwrap_or_else(|| StreamPath::from("/"))
        .to_path_buf();
    let backup = working_name(dstp, &dir, BACKUP_PREFIX)?;
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
pub(crate) fn plan_for(
    dstp: &dyn Provider,
    target: StreamPath,
    replace: bool,
    safe: bool,
) -> std::io::Result<Plan> {
    if safe {
        Plan::staged(dstp, target, replace)
    } else {
        Ok(Plan::direct(target))
    }
}

/// Publishes a finished plan. A no-op in direct mode.
pub(crate) fn publish(dstp: &dyn Provider, plan: &Plan) -> std::io::Result<()> {
    // Only local providers currently expose durable data/namespace verification.
    // Remote publication is recorded but cannot claim crash-safe resumability.
    if plan.cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "transfer cancelled",
        ));
    }
    let local = dstp.store_id() == "local";
    if let Some(review) = &plan.preflight {
        super::preflight::revalidate_destination(
            review,
            dstp,
            &plan
                .target
                .parent()
                .ok_or_else(|| std::io::Error::other("missing parent"))?,
            0,
        )
        .map_err(std::io::Error::other)?;
        if plan.is_staged()
            && super::preflight::target_stamp(dstp, &plan.target)? != plan.target_stamp
        {
            return Err(std::io::Error::other(
                "preflight target changed before commit",
            ));
        }
    }
    if let Some(step) = &plan.recovery {
        let digest = if local && plan.native_move {
            recovery::durability::rename_identity(plan.write_to.native()?)?
        } else if local {
            Some(recovery::durability::fingerprint_cancellable(
                plan.write_to.native()?,
                true,
                &plan.cancel,
            )?)
        } else {
            None
        };
        step.prepared(digest, local)
            .map_err(std::io::Error::other)?;
        step.transition(Phase::Publishing)
            .map_err(std::io::Error::other)?;
    }
    if plan.is_staged() {
        if let Some(backup) = &plan.backup {
            dstp.rename(&plan.target, backup)?;
            recovery::fault("backup");
            if local {
                recovery::durability::sync_parent(backup.native()?)?;
            }
            if let Err(error) = dstp.rename(&plan.write_to, &plan.target) {
                // Keep both references in the journal if rollback itself fails.
                dstp.rename(backup, &plan.target)?;
                if local {
                    recovery::durability::sync_parent(plan.target.native()?)?;
                }
                return Err(error);
            }
        } else {
            commit(dstp, &plan.write_to, &plan.target, plan.replace)?;
        }
    }
    if let Some(undo) = plan
        .undo
        .lock()
        .map_err(|_| std::io::Error::other("undo lock"))?
        .take()
    {
        undo.finish().map_err(std::io::Error::other)?;
    }
    recovery::fault("swapped");
    if local && plan.recovery.is_some() {
        recovery::durability::sync_parent(plan.target.native()?)?;
        if plan.native_move {
            recovery::durability::sync_parent(plan.write_to.native()?)?;
        }
    }
    if let Some(step) = &plan.recovery {
        step.transition(Phase::Committed)
            .map_err(std::io::Error::other)?;
    }
    if let Some(backup) = &plan.backup {
        dstp.remove(backup)?;
        if local {
            recovery::durability::sync_parent(backup.native()?)?;
        }
    }
    if let Some(root) = &plan.owned_root {
        std::fs::remove_dir(root.native()?)?;
        recovery::durability::sync_parent(root.native()?)?;
    }
    Ok(())
}

/// Removes the staging entry after a failed or cancelled transfer. Only
/// the staging entry — the destination is either untouched or already
/// committed. Cleanup problems are reported on their own so they are not
/// mistaken for the failure that caused them.
pub(crate) fn discard_staging(dstp: &dyn Provider, plan: &Plan, errors: &mut Vec<ItemError>) {
    if let Some(step) = &plan.recovery {
        if let Err(error) = step.cleanup_unpublished() {
            errors.push(ItemError::new(plan.write_to.display().to_string(), error));
        }
        return;
    }
    if !plan.is_staged() || !dstp.exists(&plan.write_to) {
        return;
    }
    if let Err(e) = dstp.remove(&plan.write_to) {
        errors.push(ItemError::new(
            plan.write_to.display().to_string(),
            e.into(),
        ));
    }
}

/// Working entries left in `dir` by an earlier, interrupted run. They
/// are only reported — such an entry can hold the only copy of a file,
/// so nothing here deletes anything.
pub(crate) fn stale_working_entries(provider: &dyn Provider, dir: &StreamPath) -> Vec<String> {
    let Ok(children) = provider.read_dir(dir) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|c| c.file_name()?.to_str().map(str::to_string))
        .filter(|name| is_working_entry(name))
        .collect()
}
