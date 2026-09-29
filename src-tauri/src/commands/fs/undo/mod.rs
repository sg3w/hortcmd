//! File undo is deliberately separate from native text-editing undo.
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
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

/// File undo is a beta feature and is off until it is switched on
/// (TICKET-391).
///
/// The backend keeps its own copy of the setting rather than trusting a
/// per-call flag, because the journal is written from seven commands that
/// carry no settings, and because recording is the part that costs the user
/// something: a journal entry plus, for a copy, the retained bytes next to
/// the destination. Off is therefore also the backend's default — a browser
/// build, a renderer that died before it could speak, a plugin or an MCP
/// client driving a file command all record nothing until someone says
/// otherwise.
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Pushed by the frontend at startup and on every change of the setting.
#[tauri::command]
pub fn undo_set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
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
    if !enabled() {
        return Ok(None);
    }
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
    if !enabled() {
        return Ok(Vec::new());
    }
    #[cfg(target_os = "macos")]
    return tauri::async_runtime::spawn_blocking(local::preview)
        .await
        .map_err(|_| unavailable())?;
    #[cfg(not(target_os = "macos"))]
    Ok(Vec::new())
}

#[tauri::command]
pub async fn undo_execute(id: String) -> AppResult<Vec<UndoEntry>> {
    if !enabled() {
        return Err(unavailable());
    }
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
    if !enabled() {
        return Ok(None);
    }
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
    if !enabled() {
        return Ok(());
    }
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
    if !enabled() {
        return Ok(UndoClassification::Irreversible);
    }
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
    if !enabled() {
        return Err(unavailable());
    }
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

/// The beta gate (TICKET-391), checked on every platform: the adapter behind it
/// is macOS-only, but "records nothing until switched on" is a promise the
/// whole build makes.
///
/// One test, because the flag is process-wide and a second one running beside
/// it would be reading the first one's writes.
#[cfg(test)]
mod gate_tests {
    use super::*;

    #[test]
    fn records_nothing_and_refuses_execution_until_switched_on() {
        assert!(!enabled(), "file undo must start off");

        assert!(prepare("rename", "/a", "/b").unwrap().is_none());
        assert!(prepare_group("copy", "/a", "/b", "g", false)
            .unwrap()
            .is_none());
        seal("g").unwrap();

        let preview = tauri::async_runtime::block_on(undo_preview()).unwrap();
        assert!(preview.is_empty());
        assert!(tauri::async_runtime::block_on(undo_execute("id".into())).is_err());
        assert!(tauri::async_runtime::block_on(undo_forget("id".into())).is_err());
        assert!(matches!(
            tauri::async_runtime::block_on(undo_classify(
                "copy".into(),
                vec!["/a".into()],
                "/b".into(),
                false,
            ))
            .unwrap(),
            UndoClassification::Irreversible
        ));

        undo_set_enabled(true);
        assert!(enabled());
        undo_set_enabled(false);
        assert!(!enabled());
    }
}
