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
