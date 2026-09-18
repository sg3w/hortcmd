use super::{
    broker::{Instance, PluginBroker, PluginInstance},
    manifest::{package_file, valid_id, PluginPermission, PluginRuntime},
    registry::read_manifest,
};
use std::{
    collections::VecDeque,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};
use tauri::Manager;

pub fn create(
    broker: &PluginBroker,
    root: &Path,
    plugin: &str,
    view: Option<&str>,
    window: bool,
) -> Result<PluginInstance, String> {
    if !valid_id(plugin) {
        return Err("Invalid plugin id".into());
    }
    let root = root.join(plugin);
    if std::fs::symlink_metadata(&root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Package links are forbidden".into());
    }
    let manifest = read_manifest(&root)?;
    if manifest.id != plugin || manifest.runtime != PluginRuntime::Js {
        return Err("Not a JS plugin".into());
    }
    let grants = broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?;
    let permissions = grants.get(plugin).ok_or("Plugin is not active")?.clone();
    let (entry, permission, title, singleton) = match view {
        None if window => (
            manifest.entry.current(),
            PluginPermission::Window,
            &manifest.name,
            true,
        ),
        Some(id) => {
            if window {
                return Err("Views are embedded surfaces".into());
            }
            if let Some(v) = manifest.contributes.preview.iter().find(|v| v.id == id) {
                (
                    v.entry.as_deref().or(manifest.entry.current()),
                    PluginPermission::Preview,
                    &v.title,
                    false,
                )
            } else if let Some(v) = manifest.contributes.tabs.iter().find(|v| v.id == id) {
                (
                    v.entry.as_deref().or(manifest.entry.current()),
                    PluginPermission::Tab,
                    &v.title,
                    v.singleton,
                )
            } else {
                return Err("Unknown view".into());
            }
        }
        _ => return Err("A view id is required".into()),
    };
    if !permissions.contains(&permission) || !manifest.permissions.contains(&permission) {
        return Err("Plugin surface permission denied".into());
    }
    let mut instances = broker
        .instances
        .lock()
        .map_err(|_| "Plugin registry unavailable")?;
    if singleton {
        for instance in instances.values() {
            let state = instance.lock().map_err(|_| "Plugin instance unavailable")?;
            if state.info.plugin_id == plugin
                && state.info.view_id == view.unwrap_or("window")
                && !state.info.closed
            {
                return Ok(state.info.clone());
            }
        }
    }
    if instances.len() >= 32 {
        return Err("Too many plugin instances".into());
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let id = hex::encode(bytes);
    let info = PluginInstance {
        id: id.clone(),
        plugin_id: plugin.into(),
        view_id: view.unwrap_or("window").into(),
        title: title.clone(),
        window_label: if window {
            format!("plugin-{id}")
        } else {
            "main".into()
        },
        ready: false,
        closed: false,
    };
    let entry = entry.ok_or("Missing plugin entry")?.to_string();
    instances.insert(
        id,
        Arc::new(Mutex::new(Instance {
            info: info.clone(),
            manifest,
            root,
            entry,
            permissions,
            events: VecDeque::new(),
            created: std::time::Instant::now(),
        })),
    );
    Ok(info)
}

pub fn open_window(app: &tauri::AppHandle, instance: &PluginInstance) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&instance.window_label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    let url = format!("index.html?pluginHost={}", instance.id);
    tauri::WebviewWindowBuilder::new(
        app,
        &instance.window_label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title(&instance.title)
    .inner_size(800.0, 600.0)
    .on_navigation(super::navigation_allowed)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Assets are scoped to an unguessable, live instance. Symlinks and traversal are
/// rejected on every read, including after the package has been changed on disk.
pub fn asset(
    broker: &PluginBroker,
    uri: &tauri::http::Uri,
) -> Result<(Vec<u8>, &'static str), String> {
    let route = uri.path().trim_start_matches('/');
    let (id, name) = route.split_once('/').ok_or("Invalid asset route")?;
    let instance = broker.instance(id, "main")?;
    let instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
    let entry = if name == "_entry" {
        &instance.entry
    } else {
        name
    };
    let path = package_file(&instance.root, entry)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Plugin asset exceeds 2 MiB".into());
    }
    let mime = match Path::new(entry).extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "text/plain; charset=utf-8",
    };
    if name == "_entry" {
        if !mime.starts_with("text/html") {
            return Err("JS entry must be HTML".into());
        }
        let mut document = b"<!doctype html><meta charset=utf-8><script>".to_vec();
        document.extend_from_slice(include_bytes!("sdk.js"));
        document.extend_from_slice(b"</script>");
        document.extend(bytes);
        bytes = document;
    }
    Ok((bytes, mime))
}

pub fn response(broker: &PluginBroker, uri: &tauri::http::Uri) -> tauri::http::Response<Vec<u8>> {
    let (status, body, mime) = match asset(broker, uri) {
        Ok((body, mime)) => (200, body, mime),
        Err(_) => (404, b"Plugin asset unavailable".to_vec(), "text/plain"),
    };
    // The document's CSP is independent of the application CSP. All plugin
    // HTML lives in sandboxed, opaque-origin frames with no IPC injection.
    let mut response = tauri::http::Response::new(body);
    *response.status_mut() =
        tauri::http::StatusCode::from_u16(status).unwrap_or(tauri::http::StatusCode::NOT_FOUND);
    let headers = response.headers_mut();
    headers.insert("content-type", tauri::http::HeaderValue::from_static(mime));
    headers.insert(
        "access-control-allow-origin",
        tauri::http::HeaderValue::from_static("*"),
    );
    headers.insert(
        "x-content-type-options",
        tauri::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "cache-control",
        tauri::http::HeaderValue::from_static("no-store"),
    );
    headers.insert("content-security-policy", tauri::http::HeaderValue::from_static("default-src 'none'; script-src 'unsafe-inline' hortplugin: http://hortplugin.localhost; style-src 'unsafe-inline' hortplugin: http://hortplugin.localhost; img-src data: hortplugin: http://hortplugin.localhost; font-src hortplugin: http://hortplugin.localhost; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_missing_grants_assets_and_cleanup() {
        let root =
            std::env::temp_dir().join(format!("hortcmd-js-host-{}", crate::net::unique_id()));
        let package = root.join("org.test.host");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join("index.html"),
            "<p>Hello</p><script>hortcmd.request({type:'view:ready'})</script>",
        )
        .unwrap();
        std::fs::write(package.join("plugin.json"), serde_json::json!({
            "id":"org.test.host", "name":"Test", "version":"1.0.0", "description":"Test", "author":"Test", "apiVersion":1,
            "runtime":"js", "entry":"index.html", "icon":"🌈", "permissions":["ui:window"]
        }).to_string()).unwrap();
        let broker = PluginBroker::default();
        assert!(create(&broker, &root, "org.test.host", None, true).is_err());
        broker
            .grants
            .lock()
            .unwrap()
            .insert("org.test.host".into(), vec![]);
        assert!(create(&broker, &root, "org.test.host", None, true).is_err());
        broker
            .grants
            .lock()
            .unwrap()
            .insert("org.test.host".into(), vec![PluginPermission::Window]);
        let info = create(&broker, &root, "org.test.host", None, true).unwrap();
        assert_eq!(
            create(&broker, &root, "org.test.host", None, true)
                .unwrap()
                .id,
            info.id
        );
        let uri = format!("hortplugin://localhost/{}/_entry", info.id)
            .parse()
            .unwrap();
        let (html, mime) = asset(&broker, &uri).unwrap();
        assert!(mime.starts_with("text/html"));
        assert!(String::from_utf8(html).unwrap().contains("window.hortcmd"));
        let bad = format!("hortplugin://localhost/{}/../plugin.json", info.id)
            .parse()
            .unwrap();
        assert!(asset(&broker, &bad).is_err());
        broker.close(&info.id);
        assert!(asset(&broker, &uri).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
