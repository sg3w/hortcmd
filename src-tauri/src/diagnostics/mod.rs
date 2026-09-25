//! Local, bounded diagnostics. Arbitrary payloads never enter the record schema.
mod storage;
pub mod types;
use std::{
    cell::RefCell,
    sync::{Mutex, OnceLock},
};
use types::*;
pub use types::{
    DiagnosticComponent as Component, DiagnosticEvent as Event, DiagnosticLevel as Level,
};
struct Logger {
    store: storage::Store,
    level: Level,
    key: [u8; 32],
}
static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();
thread_local! { static CONTEXT: RefCell<Option<String>> = const { RefCell::new(None) }; }
pub fn current() -> Option<String> {
    CONTEXT.with(|c| c.borrow().clone())
}
pub struct Scope(Option<String>);
impl Scope {
    pub fn enter(id: String) -> Self {
        Self(CONTEXT.with(|c| c.replace(Some(id))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CONTEXT.with(|c| c.replace(self.0.take()));
    }
}
pub fn new_id() -> String {
    let mut bytes = [0; 16];
    if getrandom::fill(&mut bytes).is_err() {
        return "00000000000000000000000000000000".into();
    }
    hex::encode(bytes)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn safe_code(code: String) -> Option<String> {
    (code.len() <= 64
        && include_str!("../errors/mod.rs").contains(&format!("\"{code}\""))
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'))
    .then_some(code)
}
fn safe_record(mut r: DiagnosticRecord) -> Option<DiagnosticRecord> {
    if !valid_id(&r.correlation) || !r.at.is_finite() || r.at < 0.0 {
        return None;
    }
    r.code = r.code.and_then(safe_code);
    r.identifier = r.identifier.filter(|v| valid_id(v));
    Some(r)
}
fn pseudonym(key: &[u8; 32], value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(key);
    h.update(value.as_bytes());
    hex::encode(&h.finalize()[..16])
}
pub fn initialize() {
    let Ok(root) = crate::config::path("diagnostics") else {
        return;
    };
    let mut key = [0; 32];
    if getrandom::fill(&mut key).is_err() {
        return;
    }
    let _ = LOGGER.set(Mutex::new(Logger {
        store: storage::Store { root },
        level: if cfg!(debug_assertions) {
            Level::Debug
        } else {
            Level::Info
        },
        key,
    }));
    emit(Component::Backend, Event::Started, Level::Info, None, None);
}
pub fn emit(
    component: Component,
    event: Event,
    level: Level,
    code: Option<&str>,
    identifier: Option<&str>,
) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    let Ok(logger) = logger.try_lock() else {
        return;
    };
    if level > logger.level {
        return;
    }
    let record = DiagnosticRecord {
        at: storage::now(),
        component,
        event,
        level,
        correlation: current().unwrap_or_else(new_id),
        code: code.map(str::to_owned),
        identifier: identifier.map(|v| pseudonym(&logger.key, v)),
    };
    let _ = logger.store.append(record); // No logging error may reach a file operation.
}
/// A deliberately small lifecycle allowlist: polls, bytes, terminal and clipboard
/// commands and authentication payloads are never inspected or recorded.
pub fn traced_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "copy_entries"
            | "move_entries"
            | "delete_entries"
            | "rename_entry"
            | "rename_batch"
            | "make_dir"
            | "net_connect"
            | "net_disconnect"
            | "net_cancel_connect"
            | "cancel_transfer"
            | "pause_transfer"
            | "pack_archive"
            | "extract_archive"
            | "plugin_open"
            | "plugin_close"
    )
}
pub fn ipc_scope(cmd: &str, id: Option<&str>) -> Option<Scope> {
    if !traced_command(cmd) {
        return None;
    }
    let scope = Scope::enter(
        id.filter(|v| valid_id(v))
            .map(str::to_owned)
            .unwrap_or_else(new_id),
    );
    emit(Component::Ipc, Event::Started, Level::Info, None, None);
    Some(scope)
}
#[tauri::command]
pub fn diagnostics_event(correlation: String, event: DiagnosticEvent, code: Option<String>) {
    if !valid_id(&correlation) {
        return;
    }
    let _scope = Scope::enter(correlation);
    emit(
        Component::Ui,
        event,
        if matches!(event, Event::Failed) {
            Level::Error
        } else {
            Level::Info
        },
        code.as_deref(),
        None,
    );
}
#[tauri::command]
pub fn diagnostics_level(level: DiagnosticLevel) {
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut l) = logger.lock() {
            l.level = level;
        }
    }
}
#[tauri::command]
pub fn diagnostics_status() -> DiagnosticStatus {
    let logger = LOGGER.get().and_then(|l| l.lock().ok());
    let mut recent = logger
        .as_ref()
        .map(|l| l.store.records())
        .unwrap_or_default();
    recent.retain(|r| r.code.is_some());
    let recent = recent.into_iter().rev().take(30).collect();
    DiagnosticStatus {
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        build: if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        }
        .into(),
        level: logger.as_ref().map(|l| l.level).unwrap_or_default(),
        location: logger
            .as_ref()
            .map(|l| l.store.root.to_string_lossy().into_owned())
            .unwrap_or_default(),
        available: logger.as_ref().is_some_and(|l| l.store.prepare().is_ok()),
        recent,
        providers: vec![
            "local: filesystem metadata, recursive size, read/write".into(),
            "sftp, ftp, ftps, smb, webdav: connected browse, preview, transfer; no recursive size"
                .into(),
        ],
        categories: vec![
            "structured lifecycle events (no payloads)".into(),
            "app version, platform, architecture, build mode".into(),
            "capability summary and diagnostics configuration".into(),
        ],
        configuration: vec![
            "retention: 4 × 512 KiB, 7 days".into(),
            "identifiers: session-keyed pseudonyms".into(),
            "telemetry: disabled (no upload implementation)".into(),
        ],
    }
}
#[tauri::command]
pub fn diagnostics_clear() -> Result<(), String> {
    if let Some(logger) = LOGGER.get() {
        let mut l = logger.lock().map_err(|_| "Diagnostics unavailable")?;
        l.store
            .clear()
            .map_err(|_| "Could not delete diagnostics")?;
        let _ = getrandom::fill(&mut l.key);
    }
    Ok(())
}
#[tauri::command]
pub fn diagnostics_export(destination: String, include_location: bool) -> Result<(), String> {
    let mut status = diagnostics_status();
    if !include_location {
        status.location.clear();
    }
    let records = LOGGER
        .get()
        .and_then(|l| l.lock().ok())
        .map(|l| l.store.records())
        .unwrap_or_default();
    let bundle = make_bundle(status, records, include_location);
    let path = std::path::Path::new(&destination);
    if !path.is_absolute() || path.extension().is_none_or(|e| e != "json") {
        return Err("Choose an absolute local .json destination".into());
    }
    write_bundle(path, &bundle)
}
fn make_bundle(
    status: DiagnosticStatus,
    records: Vec<DiagnosticRecord>,
    include_location: bool,
) -> serde_json::Value {
    serde_json::json!({"schema":1,"manifest":{"categories":status.categories,"identifiers":"session-pseudonyms","includesLogLocation":include_location,"excludes":["contents","credentials","headers","terminal","clipboard","rawErrors","paths","hosts","profileNames","usernames"]},"metadata":status,"events":records})
}
fn write_bundle(path: &std::path::Path, bundle: &serde_json::Value) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "Cannot create bundle; choose a new local filename")?;
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(&bundle).map_err(|_| "Cannot encode support bundle")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not finish support bundle".to_owned())
}
#[cfg(test)]
mod tests;
