//! File undo is deliberately separate from native text-editing undo.
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UndoEntry {
    pub id: String,
    pub group: Option<String>,
    pub kind: String,
    pub path: String,
    pub target: Option<String>,
    pub status: String,
    pub retained: Option<String>,
}

#[cfg(target_os = "macos")]
mod local;
#[cfg(all(test, target_os = "macos"))]
mod tests;

fn unavailable() -> AppError {
    AppError::new(crate::errors::code::VALIDATION_BAD_INPUT).op("file-undo")
}

pub struct Pending {
    #[cfg(target_os = "macos")]
    inner: local::Pending,
}
impl Pending {
    pub fn finish(self) -> AppResult<()> {
        #[cfg(target_os = "macos")]
        self.inner.finish()?;
        Ok(())
    }
}

/// Unsupported providers and overwrites never receive an undo entry. The plan is
/// durable before callers mutate anything; incomplete plans confer no authority.
pub fn prepare(kind: &str, source: &str, target: &str) -> AppResult<Option<Pending>> {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        local::prepare(kind, source, target).map(|p| p.map(|inner| Pending { inner }))
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let _ = (kind, source, target);
        Ok(None)
    }
}

#[tauri::command]
pub async fn undo_preview() -> AppResult<Vec<UndoEntry>> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(local::preview)
        .await
        .map_err(|_| unavailable())?;
    #[cfg(not(target_os = "macos"))]
    Ok(Vec::new())
}

#[tauri::command]
pub async fn undo_execute(id: String) -> AppResult<Vec<UndoEntry>> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || local::execute(&id))
        .await
        .map_err(|_| unavailable())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err(unavailable())
    }
}

/// One transfer or multi-rename shares a group; each completed item retains its
/// own inverse result so partial success is never presented as atomic success.
pub fn prepare_group(
    kind: &str,
    source: &str,
    target: &str,
    group: &str,
    allow_target: bool,
) -> AppResult<Option<Pending>> {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        local::prepare_group(kind, source, target, group, allow_target)
            .map(|p| p.map(|inner| Pending { inner }))
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let _ = (kind, source, target, group, allow_target);
        Ok(None)
    }
}

pub fn seal(group: &str) -> AppResult<()> {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        local::seal(group)
    }
    #[cfg(any(not(target_os = "macos"), test))]
    {
        let _ = group;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum UndoClassification {
    FullyReversible,
    Conditional,
    Irreversible,
}

/// Read-only assessment; never creates a journal entry or grants execution authority.
#[tauri::command]
pub async fn undo_classify(
    kind: String,
    sources: Vec<String>,
    target: String,
    directory: bool,
) -> AppResult<UndoClassification> {
    #[cfg(target_os = "macos")]
    {
        let eligible = tauri::async_runtime::spawn_blocking(move || {
            local::classify(&kind, &sources, &target, directory)
        })
        .await
        .map_err(|_| unavailable())??;
        Ok(if eligible {
            UndoClassification::Conditional
        } else {
            UndoClassification::Irreversible
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (kind, sources, target, directory);
        Ok(UndoClassification::Irreversible)
    }
}

#[tauri::command]
pub async fn undo_forget(id: String) -> AppResult<Vec<UndoEntry>> {
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(move || local::forget(&id))
        .await
        .map_err(|_| unavailable())?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = id;
        Err(unavailable())
    }
}
