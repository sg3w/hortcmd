//! Thin IPC adapters; storage and recovery decisions remain in the backend.
use super::{journal, reconcile, types::*};
use crate::errors::{code, AppError, AppResult};

#[tauri::command]
pub async fn recovery_queue_transfer(
    id: String,
    kind: RecoveryKind,
    sources: Vec<String>,
    destination: String,
    settings: RecoverySettings,
    review: Option<String>,
) -> AppResult<()> {
    if !super::supported() {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if !matches!(kind, RecoveryKind::Copy | RecoveryKind::Move) {
            return Err(AppError::new(code::VALIDATION_BAD_INPUT));
        }
        let plan = super::super::preflight::build(
            kind.clone(),
            &sources,
            &destination,
            settings.clone(),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        plan.ensure_executable()?;
        if let Some(review) = review {
            plan.matches_review(&review)?;
        }
        // Beta and off by default (TICKET-396). The preflight above is a
        // safety check on the transfer itself and runs either way; only the
        // journal — the part that outlives the operation — is withheld.
        if !super::enabled() {
            return Ok(());
        }
        let journal = journal()?;
        journal.enqueue(
            &id,
            Intent::transfer(kind, &sources, &destination, settings)?,
        )?;
        journal.attach_preflight(&id, plan)
    })
    .await
    .map_err(|_| AppError::new(code::FS_IO))?
}

#[tauri::command]
pub async fn recovery_list() -> AppResult<Vec<RecoveryEntry>> {
    if !super::supported() || !super::enabled() {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let journal = journal()?;
        Ok(journal
            .records()?
            .iter()
            .filter(|r| !r.acknowledged)
            .map(|r| reconcile::entry(&journal, r))
            .collect())
    })
    .await
    .map_err(|_| AppError::new(code::FS_IO))?
}

#[tauri::command]
pub async fn recovery_prepare(id: String, resume: bool) -> AppResult<RecoveryLaunch> {
    // Restarting a journaled job is the one mutating step behind the switch:
    // a window left open across a settings change, or any non-UI caller, is
    // turned away rather than trusted (TICKET-396). Cleanup, discard and
    // acknowledge keep working — they only ever remove a record.
    if !super::enabled() {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("transfer-recovery"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let journal = journal()?;
        let record = journal
            .records()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))?;
        let intent = reconcile::retry_intent(&journal, &record, resume)?;
        let sources = intent
            .sources
            .iter()
            .map(RecoveryLocation::resolve)
            .collect::<AppResult<Vec<_>>>()?;
        let destination = intent.destination.resolve()?;
        let new_id = crate::net::unique_id();
        journal.enqueue_retry(&new_id, intent.clone(), Some(id))?;
        Ok(RecoveryLaunch {
            id: new_id,
            kind: intent.kind,
            sources,
            destination,
            base: intent.base,
            names: intent.names,
            settings: intent.settings,
        })
    })
    .await
    .map_err(|_| AppError::new(code::FS_IO))?
}

#[tauri::command]
pub async fn recovery_cleanup(id: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let journal = journal()?;
        let record = journal
            .records()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))?;
        reconcile::cleanup(&journal, &record)
    })
    .await
    .map_err(|_| AppError::new(code::FS_IO))?
}

#[tauri::command]
pub async fn recovery_discard(id: String) -> AppResult<()> {
    if !super::supported() {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || journal()?.discard(&id))
        .await
        .map_err(|_| AppError::new(code::FS_IO))?
}

#[tauri::command]
pub async fn recovery_acknowledge(id: String) -> AppResult<()> {
    if !super::supported() {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || journal()?.acknowledge(&id))
        .await
        .map_err(|_| AppError::new(code::FS_IO))?
}

/// Reveal resolves the current verified profile, but never connects or opens an
/// executable/file. The frontend navigates a pane to this directory.
#[tauri::command]
pub fn recovery_destination(id: String) -> AppResult<String> {
    let journal = journal()?;
    let record = journal
        .records()?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))?;
    let destination = record.intent.destination.resolve()?;
    if record.intent.kind == RecoveryKind::Pack {
        let (provider, path) = crate::vfs::resolve(&destination)?;
        let parent = path
            .parent()
            .ok_or_else(|| AppError::new(code::VALIDATION_BAD_INPUT))?;
        let _ = provider;
        record.intent.destination.with_path(&parent)?.resolve()
    } else {
        Ok(destination)
    }
}
