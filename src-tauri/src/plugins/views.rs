use super::{
    broker::{PluginBroker, PluginInstance},
    js_host,
    manifest::{PluginPermission, PluginViewKind},
    protocol::PluginSide,
    registry,
};
use std::path::Path;
use tauri::Manager;
pub fn preview(
    app: &tauri::AppHandle,
    root: &Path,
    path: &str,
    side: PluginSide,
) -> Result<Option<PluginInstance>, String> {
    let broker = app.state::<PluginBroker>();
    let grants = broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?
        .clone();
    let manifests: Vec<_> = registry::discover(root)?
        .into_iter()
        .filter_map(|r| r.manifest)
        .filter(|m| {
            grants
                .get(&m.id)
                .is_some_and(|p| p.contains(&PluginPermission::Preview))
        })
        .collect();
    if manifests.is_empty() {
        return Ok(None);
    }
    let (provider, resolved) = crate::vfs::resolve(path).map_err(|e| e.to_string())?;
    let stat = provider.stat(&resolved).map_err(|e| e.to_string())?;
    let kind = if stat.is_dir {
        PluginViewKind::Folder
    } else {
        PluginViewKind::File
    };
    let location = crate::vfs::location::VfsLocation::parse(path).map_err(|e| e.to_string())?;
    let filename = location.path();
    let extension = Path::new(&filename).extension().and_then(|e| e.to_str());
    let mime = extension.map(|e| match e.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html",
        "php" => "text/x-php",
        "md" => "text/markdown",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    });
    let Some((manifest, view)) = registry::preview_match(&manifests, &kind, extension, mime) else {
        return Ok(None);
    };
    js_host::create(
        &broker,
        root,
        &manifest.id,
        Some(&view.id),
        false,
        side,
        Some(path),
    )
    .map(Some)
}
