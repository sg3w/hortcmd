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
use crate::vfs::Provider;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Builds the target path in the target folder from the source base name.
pub(crate) fn target_in(dest_dir: &Path, src: &Path) -> Option<PathBuf> {
    src.file_name().map(|name| dest_dir.join(name))
}

/// Counts files and bytes recursively (for the progress totals).
pub(crate) fn measure(provider: &dyn Provider, p: &Path) -> (u32, u64) {
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

/// Finds a free name "Name (2)", "Name (3)" … in the target folder.
pub(crate) fn free_name(provider: &dyn Provider, target: &Path) -> PathBuf {
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
pub(crate) fn working_name(provider: &dyn Provider, dir: &Path, prefix: &str) -> PathBuf {
    /// Distinguishes working entries created in the same millisecond by
    /// the same process; the existence check below closes the rest of
    /// the gap. Its own counter — sharing the collision-request counter
    /// was incidental, and coupling two unrelated things through one
    /// global is exactly what this split is for.
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let pid = std::process::id();
    loop {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let cand = dir.join(format!("{}{}-{}", prefix, pid, n));
        if !provider.exists(&cand) {
            return cand;
        }
    }
}

/// How one top-level source reaches its final name.
pub(crate) struct Plan {
    /// Where the bytes are written: the staging entry in safe mode, the
    /// final name in direct mode.
    pub(crate) write_to: PathBuf,
    /// The name the user asked for.
    pub(crate) target: PathBuf,
    /// The final name still exists and has to be replaced on commit.
    /// Only ever true in safe mode — direct mode deletes it up front.
    pub(crate) replace: bool,
}

impl Plan {
    /// Direct mode: the bytes already go to their final name.
    pub(crate) fn direct(target: PathBuf) -> Self {
        Plan {
            write_to: target.clone(),
            target,
            replace: false,
        }
    }

    /// Safe mode: a hidden sibling of the target receives the bytes.
    pub(crate) fn staged(provider: &dyn Provider, target: PathBuf, replace: bool) -> Self {
        let dir = target.parent().unwrap_or(Path::new("/")).to_path_buf();
        Plan {
            write_to: working_name(provider, &dir, STAGE_PREFIX),
            target,
            replace,
        }
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
pub(crate) fn plan_for(dstp: &dyn Provider, target: PathBuf, replace: bool, safe: bool) -> Plan {
    if safe {
        Plan::staged(dstp, target, replace)
    } else {
        Plan::direct(target)
    }
}

/// Publishes a finished plan. A no-op in direct mode.
pub(crate) fn publish(dstp: &dyn Provider, plan: &Plan) -> std::io::Result<()> {
    if !plan.is_staged() {
        return Ok(());
    }
    commit(dstp, &plan.write_to, &plan.target, plan.replace)
}

/// Removes the staging entry after a failed or cancelled transfer. Only
/// the staging entry — the destination is either untouched or already
/// committed. Cleanup problems are reported on their own so they are not
/// mistaken for the failure that caused them.
pub(crate) fn discard_staging(dstp: &dyn Provider, plan: &Plan, errors: &mut Vec<ItemError>) {
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
pub(crate) fn stale_working_entries(provider: &dyn Provider, dir: &Path) -> Vec<String> {
    let Ok(children) = provider.read_dir(dir) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|c| c.file_name()?.to_str().map(str::to_string))
        .filter(|name| is_working_entry(name))
        .collect()
}
