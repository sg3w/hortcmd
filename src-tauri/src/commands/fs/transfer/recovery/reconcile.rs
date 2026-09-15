//! Read-only startup reconciliation. Existence alone never proves a completed copy.
use super::{durability, types::*, Journal};
use crate::errors::{code, AppError, AppResult};
use crate::vfs;
use std::path::Path;

fn exists(location: &RecoveryLocation) -> AppResult<bool> {
    let address = location.resolve()?;
    let (provider, path) = vfs::resolve(&address)?;
    Ok(provider.exists(&path))
}

pub fn verified(item: &Item) -> bool {
    match (&item.target, &item.digest) {
        (RecoveryLocation::Local { path }, Some(digest)) if item.durable => {
            durability::matches(Path::new(path), digest).unwrap_or(false)
        }
        _ => false,
    }
}

pub fn entry(journal: &Journal, record: &Record) -> RecoveryEntry {
    let mut view = RecoveryEntry {
        id: record.id.clone(),
        kind: record.intent.kind.clone(),
        state: RecoveryState::Retryable,
        order: record.order,
        sources: record.intent.sources.clone(),
        destination: record.intent.destination.clone(),
        committed: 0,
        total: record.intent.sources.len() as u32,
        reason: record.reason.clone(),
    };
    if journal.active(&record.id) {
        view.state = RecoveryState::Running;
        return view;
    }
    if !record.started {
        view.state = RecoveryState::Queued;
        return view;
    }
    // A durable terminal success describes history, not authority to modify the
    // current destination. No rehash/network read is needed just to acknowledge it.
    if record.terminal && record.reason.is_none() {
        view.state = RecoveryState::Completed;
        view.committed = record
            .items
            .iter()
            .filter(|i| {
                i.source.is_some() && matches!(i.phase, Phase::Committed | Phase::SourceRemoved)
            })
            .count() as u32;
        if matches!(
            record.intent.kind,
            RecoveryKind::Extract | RecoveryKind::Pack
        ) {
            view.committed = view.total;
        }
        return view;
    }
    if record
        .intent
        .sources
        .iter()
        .chain(std::iter::once(&record.intent.destination))
        .any(|l| l.resolve().is_err())
    {
        view.state = RecoveryState::Unavailable;
        return view;
    }
    let mut leftovers = false;
    let mut uncertain = false;
    let mut pending_source = false;
    let mut archive_complete = false;
    for item in &record.items {
        if item.phase == Phase::Skipped {
            continue;
        }
        for artifact in item
            .staging
            .iter()
            .chain(item.backup.iter())
            .chain(item.owner.iter().map(|owner| &owner.location))
        {
            match exists(artifact) {
                Ok(true) => leftovers = true,
                Err(_) => uncertain = true,
                Ok(false) => {}
            }
        }
        if item.source.is_none() && item.owner.is_some() {
            archive_complete |= item.phase == Phase::Committed;
            continue;
        }
        if matches!(
            item.phase,
            Phase::Publishing | Phase::Committed | Phase::SourceRemoved
        ) {
            if verified(item) {
                if item.source.is_some() {
                    view.committed += 1;
                }
                if record.intent.kind == RecoveryKind::Move {
                    pending_source |= item
                        .source
                        .as_ref()
                        .is_some_and(|s| exists(s).unwrap_or(true));
                }
            } else {
                uncertain = true;
            }
        }
    }
    if archive_complete && !uncertain {
        view.committed = view.total;
    }
    view.state = if !record.intent.settings.safe && !record.terminal {
        RecoveryState::UnsafeInterrupted
    } else if leftovers || pending_source {
        RecoveryState::CleanupRequired
    } else if uncertain {
        RecoveryState::Unrecoverable
    } else if view.committed as usize == record.intent.sources.len() || archive_complete {
        RecoveryState::Completed
    } else if view.committed > 0
        && matches!(record.intent.kind, RecoveryKind::Copy | RecoveryKind::Move)
    {
        RecoveryState::Resumable
    } else {
        RecoveryState::Retryable
    };
    view
}

pub fn retry_intent(journal: &Journal, record: &Record, resume: bool) -> AppResult<Intent> {
    let view = entry(journal, record);
    if matches!(
        view.state,
        RecoveryState::Running
            | RecoveryState::Unavailable
            | RecoveryState::CleanupRequired
            | RecoveryState::Unrecoverable
            | RecoveryState::Completed
    ) || (resume && !matches!(view.state, RecoveryState::Queued | RecoveryState::Resumable))
    {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("transfer-recovery"));
    }
    let mut intent = record.intent.clone();
    if resume {
        intent.sources.retain(|source| {
            !record.items.iter().any(|item| {
                item.source.as_ref() == Some(source)
                    && matches!(item.phase, Phase::Committed | Phase::SourceRemoved)
                    && verified(item)
                    && (intent.kind != RecoveryKind::Move || item.phase == Phase::SourceRemoved)
            })
        });
    }
    // Every remaining source must still exist through the live provider. Nothing
    // connects, deletes a move source, or grants an old overwrite decision here.
    for source in &intent.sources {
        if !exists(source)? {
            return Err(AppError::new(code::FS_NOT_FOUND));
        }
    }
    Ok(intent)
}

/// Cleans only recorded private staging directories whose identity still matches.
/// A backup may be removed only after its replacement verifies. Sources and final
/// destinations are never cleanup targets.
pub fn cleanup(journal: &Journal, record: &Record) -> AppResult<()> {
    if journal.active(&record.id) {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT));
    }
    for item in &record.items {
        if item
            .backup
            .as_ref()
            .is_some_and(|p| exists(p).unwrap_or(true))
            && !verified(item)
        {
            return Err(AppError::new(code::TRANSFER_COMMIT).op("recovery-preserve-backup"));
        }
    }
    let mut roots = std::collections::BTreeSet::new();
    for owner in record.items.iter().filter_map(|item| item.owner.as_ref()) {
        let RecoveryLocation::Local { path } = &owner.location else {
            continue;
        };
        let path = Path::new(path);
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with(".hortcmd-stage-") && !name.starts_with(".hortcmd-extract-") {
            return Err(AppError::new(code::VALIDATION_BAD_INPUT));
        }
        if !path.try_exists()? {
            continue;
        }
        if durability::object_identity(path)?.as_deref() != Some(&owner.identity) {
            return Err(AppError::new(code::TRANSFER_COMMIT).op("recovery-staging-changed"));
        }
        roots.insert(path.to_path_buf());
    }
    if roots.is_empty() {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("recovery-no-owned-staging"));
    }
    for root in roots {
        std::fs::remove_dir_all(&root)?;
        durability::sync_parent(&root)?;
    }
    Ok(())
}
