//! A launcher accepts only a declared contribution ID, never an arbitrary action.
use super::{
    broker::{PluginBroker, PluginInstance},
    js_host,
    manifest::{valid_id, PluginAction, PluginPermission, PluginRuntime},
    native_host,
    protocol::PluginEvent,
    registry::read_manifest,
};
use std::path::Path;
use tauri::Manager;

pub fn launch(
    app: &tauri::AppHandle,
    root: &Path,
    plugin: &str,
    button: &str,
) -> Result<PluginInstance, String> {
    if !valid_id(plugin) {
        return Err("Invalid plugin id".into());
    }
    let manifest = read_manifest(&root.join(plugin))?;
    let broker = app.state::<PluginBroker>();
    let permissions = broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?
        .get(plugin)
        .ok_or("Plugin is inactive")?
        .clone();
    if manifest.id != plugin
        || !permissions.contains(&PluginPermission::Commandbar)
        || !manifest.permissions.contains(&PluginPermission::Commandbar)
    {
        return Err("Plugin command-bar permission denied".into());
    }
    let button = manifest
        .contributes
        .commandbar
        .iter()
        .find(|b| b.id == button)
        .ok_or("Unknown plugin button")?;
    match &button.action {
        PluginAction::OpenWindow => {
            let info = js_host::create(
                &broker,
                root,
                plugin,
                None,
                true,
                super::protocol::PluginSide::Left,
                None,
            )?;
            if let Err(error) = js_host::open_window(app, &info) {
                broker.close(&info.id);
                return Err(error);
            }
            Ok(info)
        }
        PluginAction::Command { command } => {
            let info = if manifest.runtime == PluginRuntime::Native {
                native_host::start(app, root, plugin)?
            } else {
                let instances = broker
                    .instances
                    .lock()
                    .map_err(|_| "Plugin registry unavailable")?;
                let running = instances
                    .values()
                    .filter_map(|s| s.lock().ok())
                    .find(|s| s.info.plugin_id == plugin && !s.info.closed)
                    .map(|s| s.info.clone());
                drop(instances);
                match running {
                    Some(info) => info,
                    None if permissions.contains(&PluginPermission::Window) => {
                        let info = js_host::create(
                            &broker,
                            root,
                            plugin,
                            None,
                            true,
                            super::protocol::PluginSide::Left,
                            None,
                        )?;
                        if let Err(error) = js_host::open_window(app, &info) {
                            broker.close(&info.id);
                            return Err(error);
                        }
                        info
                    }
                    None => return Err("Open a plugin view before sending this command".into()),
                }
            };
            broker
                .instance(&info.id, "main")?
                .lock()
                .map_err(|_| "Plugin instance unavailable")?
                .push(PluginEvent::Command {
                    command: command.clone(),
                });
            Ok(info)
        }
    }
}
