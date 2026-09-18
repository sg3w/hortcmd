//! Plugin IPC accepts calls from the trusted application window only.
use crate::plugins::registry::{self, PluginRecord};
use tauri::Manager;

pub fn trusted(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Plugin administration requires the main window".into())
    }
}
pub fn root(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("plugins"))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn list_plugins(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Vec<PluginRecord>, String> {
    trusted(&window)?;
    registry::discover(&root(&app)?)
}

use crate::plugins::{
    broker::{PluginBroker, PluginInstance},
    js_host,
    protocol::{PluginEvent, PluginRequest},
};

#[tauri::command]
pub fn plugin_open(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    plugin: String,
    view: Option<String>,
    separate: bool,
) -> Result<PluginInstance, String> {
    trusted(&window)?;
    let instance = js_host::create(&state, &root(&app)?, &plugin, view.as_deref(), separate)?;
    if separate {
        if let Err(error) = js_host::open_window(&app, &instance) {
            state.close(&instance.id);
            return Err(error);
        }
    }
    Ok(instance)
}
#[tauri::command]
pub fn plugin_info(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    id: String,
) -> Result<PluginInstance, String> {
    let instance = state.instance(&id, window.label())?;
    let info = instance
        .lock()
        .map_err(|_| "Plugin instance unavailable")?
        .info
        .clone();
    Ok(info)
}
#[tauri::command]
pub fn plugin_request(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    id: String,
    request: PluginRequest,
) -> Result<serde_json::Value, String> {
    state.control(&id, window.label(), request)
}
#[tauri::command]
pub fn plugin_poll(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    id: String,
) -> Result<Vec<PluginEvent>, String> {
    let instance = state.instance(&id, window.label())?;
    let mut instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
    instance.authorize(&PluginRequest::Ping)?;
    if !instance.info.ready {
        return Ok(vec![]);
    }
    Ok(instance.events.drain(..).collect())
}
#[tauri::command]
pub fn plugin_close(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    id: String,
) -> Result<(), String> {
    // Closed instances remain removable after view:requestClose.
    let label = {
        let instances = state
            .instances
            .lock()
            .map_err(|_| "Plugin registry unavailable")?;
        let Some(instance) = instances.get(&id) else {
            return Ok(());
        };
        let instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        if window.label() != "main" && window.label() != instance.info.window_label {
            return Err("Wrong instance owner".into());
        }
        instance.info.window_label.clone()
    };
    state.close(&id);
    if label != "main" {
        if let Some(window) = app.get_webview_window(&label) {
            window.close().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn plugin_native_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
) -> Result<PluginInstance, String> {
    trusted(&window)?;
    crate::plugins::native_host::start(&app, &root(&app)?, &plugin)
}
