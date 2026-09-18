use super::{
    broker::PluginBroker,
    protocol::PluginRequest,
    transfers::{self, PluginTransferKind},
};
use crate::vfs::{
    self,
    caps::{self, Capability},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::Manager;
use ts_rs::TS;
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginUiEvent {
    Notify { plugin: String, message: String },
    Open { path: String },
    Limit { value: u32 },
}
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginUiState {
    pub events: Vec<PluginUiEvent>,
    pub transfers: Vec<super::transfers::PluginTransfer>,
}
pub fn validate_path(path: &str) -> Result<(), String> {
    let location = vfs::location::VfsLocation::parse(path).map_err(|e| e.to_string())?;
    if location.scheme() == vfs::Scheme::Local
        && !std::path::Path::new(&location.path()).is_absolute()
    {
        return Err("Plugin file paths must be absolute".into());
    }
    Ok(())
}
fn ui(broker: &PluginBroker, event: PluginUiEvent) -> Result<(), String> {
    let mut queue = broker
        .ui
        .lock()
        .map_err(|_| "Plugin UI queue unavailable")?;
    if queue.len() >= 128 {
        return Err("Plugin UI queue is full".into());
    }
    queue.push_back(event);
    Ok(())
}
pub fn poll(broker: &PluginBroker) -> Result<PluginUiState, String> {
    let transfers = transfers::pending(broker)?;
    let events = broker
        .ui
        .lock()
        .map_err(|_| "Plugin UI queue unavailable")?
        .drain(..)
        .collect();
    Ok(PluginUiState { events, transfers })
}
/// Filesystem services are called only after the instance and capability check.
/// Call on a blocking worker; network providers may block within their deadlines.
pub fn dispatch(
    app: &tauri::AppHandle,
    id: &str,
    caller: &str,
    request: PluginRequest,
) -> Result<Value, String> {
    let broker = app.state::<PluginBroker>();
    let instance = broker.instance(id, caller)?;
    instance
        .lock()
        .map_err(|_| "Plugin instance unavailable")?
        .authorize(&request)?;
    let result = match &request {
        PluginRequest::Read { .. }
        | PluginRequest::List { .. }
        | PluginRequest::Preview { .. }
        | PluginRequest::Write { .. } => super::filesystem::execute(&request),
        PluginRequest::Open { path } => {
            validate_path(path)?;
            caps::require(path, Capability::Browse).map_err(|e| e.to_string())?;
            let (provider, resolved) = vfs::resolve(path).map_err(|e| e.to_string())?;
            if !provider.stat(&resolved).map_err(|e| e.to_string())?.is_dir {
                return Err("op:open navigates directories only; it never executes files".into());
            }
            ui(&broker, PluginUiEvent::Open { path: path.clone() })?;
            Ok(Value::Null)
        }
        PluginRequest::Copy {
            sources,
            destination,
        }
        | PluginRequest::Move {
            sources,
            destination,
        } => {
            let kind = if matches!(request, PluginRequest::Move { .. }) {
                PluginTransferKind::Move
            } else {
                PluginTransferKind::Copy
            };
            let transfer = transfers::enqueue(
                &broker,
                id,
                caller,
                kind,
                sources.clone(),
                destination.clone(),
            )?;
            Ok(json!({"operationId":transfer.id,"status":"awaiting-confirmation"}))
        }
        PluginRequest::Limit { value } => {
            let mut workspace = broker
                .workspace
                .lock()
                .map_err(|_| "Plugin context unavailable")?;
            let workspace = workspace
                .as_mut()
                .ok_or("No copy-limit snapshot available")?;
            if let Some(value) = value {
                ui(&broker, PluginUiEvent::Limit { value: *value })?;
                *broker
                    .limit_override
                    .lock()
                    .map_err(|_| "Plugin limit unavailable")? = Some(*value);
                workspace.left.copy_limit = *value;
                workspace.right.copy_limit = *value;
            }
            Ok(json!(workspace.left.copy_limit))
        }
        PluginRequest::Notify { message } => {
            if message.is_empty() || message.len() > 4096 {
                return Err("Notification must contain 1 to 4096 bytes".into());
            }
            let plugin = instance
                .lock()
                .map_err(|_| "Plugin instance unavailable")?
                .manifest
                .name
                .clone();
            ui(
                &broker,
                PluginUiEvent::Notify {
                    plugin,
                    message: message.clone(),
                },
            )?;
            Ok(Value::Null)
        }
        _ => broker.control(id, caller, request.clone()),
    }?;
    // A revoked instance cannot receive a read result that completed later.
    if !matches!(request, PluginRequest::Close) {
        instance
            .lock()
            .map_err(|_| "Plugin instance unavailable")?
            .authorize(&request)?;
    }
    if let PluginRequest::Title { title } = request {
        let label = instance
            .lock()
            .map_err(|_| "Plugin instance unavailable")?
            .info
            .window_label
            .clone();
        if label != "main" {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.set_title(&title);
            }
        }
    }
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > 768 * 1024
    {
        return Err("Plugin response exceeds 768 KiB".into());
    }
    Ok(result)
}
