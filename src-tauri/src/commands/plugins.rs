//! Plugin IPC accepts calls from the trusted application window only.
use crate::plugins::registry::PluginRecord;
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
pub async fn list_plugins(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Vec<PluginRecord>, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let records =
            crate::plugins::management::records(&app.state::<PluginBroker>(), &root(&app)?)?;
        close_orphan_windows(&app);
        Ok(records)
    })
    .await
    .map_err(|e| e.to_string())?
}

use crate::plugins::{
    broker::{PluginBroker, PluginInstance},
    js_host,
    protocol::{PluginEvent, PluginRequest},
};

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn plugin_open(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
    view: Option<String>,
    separate: bool,
    side: crate::plugins::protocol::PluginSide,
    path: Option<String>,
) -> Result<PluginInstance, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PluginBroker>();

        crate::plugins::management::records(&state, &root(&app)?)?;
        let instance = js_host::create(
            &state,
            &root(&app)?,
            &plugin,
            view.as_deref(),
            separate,
            side,
            path.as_deref(),
        )?;
        if separate {
            if let Err(error) = js_host::open_window(&app, &instance) {
                state.close(&instance.id);
                return Err(error);
            }
        }
        Ok(instance)
    })
    .await
    .map_err(|e| e.to_string())?
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
pub async fn plugin_request(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    request: PluginRequest,
) -> Result<serde_json::Value, String> {
    let caller = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::actions::dispatch(&app, &id, &caller, request)
    })
    .await
    .map_err(|e| e.to_string())?
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
pub async fn plugin_native_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
) -> Result<PluginInstance, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::records(&app.state::<PluginBroker>(), &root(&app)?)?;
        crate::plugins::native_host::start(&app, &root(&app)?, &plugin)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn plugin_launch(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
    button: String,
) -> Result<PluginInstance, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::records(&app.state::<PluginBroker>(), &root(&app)?)?;
        crate::plugins::launch::launch(&app, &root(&app)?, &plugin, &button)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn plugin_workspace(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    workspace: crate::plugins::context::PluginWorkspace,
) -> Result<(), String> {
    trusted(&window)?;
    crate::plugins::context::publish(&state, workspace)
}
#[tauri::command]
pub async fn plugin_preview(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    path: String,
    side: crate::plugins::protocol::PluginSide,
) -> Result<Option<PluginInstance>, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::records(&app.state::<PluginBroker>(), &root(&app)?)?;
        crate::plugins::views::preview(&app, &root(&app)?, &path, side)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn plugin_ui_poll(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
) -> Result<crate::plugins::actions::PluginUiState, String> {
    trusted(&window)?;
    crate::plugins::actions::poll(&state)
}
#[tauri::command]
pub async fn plugin_transfer_start(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    id: String,
    settings: crate::commands::fs::transfer::recovery::types::RecoverySettings,
    review: String,
) -> Result<(), String> {
    trusted(&window)?;
    crate::plugins::transfers::start(&app, &id, settings, review).await
}
#[tauri::command]
pub fn plugin_transfer_cancel(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, PluginBroker>,
    id: String,
) -> Result<(), String> {
    trusted(&window)?;
    crate::plugins::transfers::cancel(&state, &id)
}

#[tauri::command]
pub async fn plugin_install(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    source: String,
    update: Option<String>,
) -> Result<String, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::install(
            &app.state::<PluginBroker>(),
            &root(&app)?,
            std::path::Path::new(&source),
            update.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn plugin_activate(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
    fingerprint: String,
) -> Result<(), String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::activate(
            &app.state::<PluginBroker>(),
            &root(&app)?,
            &plugin,
            &fingerprint,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn plugin_deactivate(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    plugin: String,
    remove: bool,
) -> Result<(), String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::plugins::management::deactivate(
            &app.state::<PluginBroker>(),
            &root(&app)?,
            &plugin,
            remove,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

fn close_orphan_windows(app: &tauri::AppHandle) {
    let broker = app.state::<PluginBroker>();
    let labels: Vec<_> = broker
        .instances
        .lock()
        .map(|instances| {
            instances
                .values()
                .filter_map(|instance| instance.lock().ok().map(|i| i.info.window_label.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (label, window) in app.webview_windows() {
        if label.starts_with("plugin-") && !labels.contains(&label) {
            let _ = window.close();
        }
    }
}
