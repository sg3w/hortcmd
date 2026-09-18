//! Context comes only from the trusted UI; paths are canonicalized by the VFS.
use super::{
    broker::{Instance, PluginBroker},
    manifest::{PluginPermission, PluginTabPath},
    protocol::{PluginContext, PluginEvent, PluginSide, PluginViewOpen, PluginViewPath},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginWorkspace {
    pub active: PluginSide,
    pub left: PluginContext,
    pub right: PluginContext,
}
#[derive(Clone, Default)]
pub enum ContextBinding {
    #[default]
    Current,
    Pane(PluginSide),
    Preview {
        side: PluginSide,
        path: String,
    },
    Visual,
}
fn canonical(path: &str) -> Result<String, String> {
    let location = crate::vfs::location::VfsLocation::parse(path).map_err(|e| e.to_string())?;
    if location.scheme() == crate::vfs::Scheme::Local
        && !std::path::Path::new(&location.path()).is_absolute()
    {
        return Err("Plugin context requires absolute paths".into());
    }
    Ok(location.uri())
}
impl PluginWorkspace {
    pub fn normalize(mut self) -> Result<Self, String> {
        if serde_json::to_vec(&self).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("Plugin workspace exceeds 256 KiB".into());
        }
        self.left.side = PluginSide::Left;
        self.right.side = PluginSide::Right;
        for context in [&mut self.left, &mut self.right] {
            context.path = canonical(&context.path)?;
            context.folder_path = canonical(&context.folder_path)?;
            context.file_path = context.file_path.as_deref().map(canonical).transpose()?;
            if context.selection.len() > 4096 {
                return Err("Too many selected files".into());
            }
            context.selection = context
                .selection
                .iter()
                .map(|p| canonical(p))
                .collect::<Result<_, _>>()?;
            if let Some(operation) = &mut context.copy_operation {
                if operation.sources.len() > 4096 {
                    return Err("Too many copy sources".into());
                }
                operation.sources = operation
                    .sources
                    .iter()
                    .map(|p| canonical(p))
                    .collect::<Result<_, _>>()?;
                operation.destination = canonical(&operation.destination)?;
            }
        }
        Ok(self)
    }
    pub fn pane(&self, side: PluginSide) -> &PluginContext {
        match side {
            PluginSide::Left => &self.left,
            PluginSide::Right => &self.right,
        }
    }
}
pub fn value(instance: &Instance, workspace: Option<&PluginWorkspace>) -> Option<PluginContext> {
    if !instance
        .permissions
        .contains(&PluginPermission::ReadContext)
    {
        return None;
    }
    let workspace = workspace?;
    match instance.binding {
        ContextBinding::Current => Some(workspace.pane(workspace.active).clone()),
        ContextBinding::Pane(side) | ContextBinding::Preview { side, .. } => {
            Some(workspace.pane(side).clone())
        }
        ContextBinding::Visual => None,
    }
}
pub fn path(instance: &Instance, workspace: Option<&PluginWorkspace>) -> Option<String> {
    let context = value(instance, workspace)?;
    match &instance.binding {
        ContextBinding::Preview { path, .. } => Some(path.clone()),
        _ => Some(context.path),
    }
}
pub fn initialize(instance: &mut Instance, workspace: Option<&PluginWorkspace>) {
    instance.push(PluginEvent::Open {
        data: PluginViewOpen {
            view_id: instance.info.view_id.clone(),
            instance_id: instance.info.id.clone(),
            path: path(instance, workspace),
            context: value(instance, workspace),
        },
    });
}
pub fn bind_view(
    instance: &mut Instance,
    side: PluginSide,
    path_override: Option<&str>,
) -> Result<(), String> {
    instance.binding = if instance
        .manifest
        .contributes
        .preview
        .iter()
        .any(|v| v.id == instance.info.view_id)
    {
        ContextBinding::Preview {
            side,
            path: canonical(path_override.ok_or("Preview path is required")?)?,
        }
    } else if let Some(tab) = instance
        .manifest
        .contributes
        .tabs
        .iter()
        .find(|v| v.id == instance.info.view_id)
    {
        if tab.path == PluginTabPath::Active {
            ContextBinding::Pane(side)
        } else {
            ContextBinding::Visual
        }
    } else {
        ContextBinding::Current
    };
    Ok(())
}
pub fn publish(broker: &PluginBroker, workspace: PluginWorkspace) -> Result<(), String> {
    let workspace = workspace.normalize()?;
    let previous = broker
        .workspace
        .lock()
        .map_err(|_| "Plugin context unavailable")?
        .replace(workspace.clone());
    let instances = broker
        .instances
        .lock()
        .map_err(|_| "Plugin registry unavailable")?;
    for instance in instances.values() {
        let mut instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        if instance.info.closed {
            continue;
        }
        if matches!(instance.binding, ContextBinding::Pane(_)) {
            let next = path(&instance, Some(&workspace));
            if next != path(&instance, previous.as_ref()) {
                if let Some(path) = next {
                    let instance_id = instance.info.id.clone();
                    instance.push(PluginEvent::Path {
                        data: PluginViewPath { instance_id, path },
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{
        broker::PluginInstance,
        manifest::PluginManifest,
        protocol::{PluginCopyOperation, PluginRequest},
    };
    use super::*;
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Instant,
    };
    fn workspace() -> PluginWorkspace {
        let context = PluginContext {
            path: "/folder".into(),
            folder_path: "/folder".into(),
            file_path: Some("/folder/file.php".into()),
            selection: vec!["/folder/file.php".into()],
            side: PluginSide::Left,
            copy_limit: 512,
            copy_operation: Some(PluginCopyOperation {
                sources: vec!["/folder/file.php".into()],
                destination: "/target".into(),
            }),
        };
        PluginWorkspace {
            active: PluginSide::Left,
            left: context.clone(),
            right: PluginContext {
                path: "sftp://user@host:22/photos".into(),
                folder_path: "sftp://user@host:22/photos".into(),
                file_path: None,
                selection: vec![],
                side: PluginSide::Right,
                ..context
            },
        }
    }
    fn instance() -> Instance {
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({"id":"org.test.context", "name":"Test", "version":"1.0.0", "description":"Test", "author":"Test", "apiVersion":1,"runtime":"js","entry":"index.html","icon":"🌈","permissions":["read:context"]})).unwrap();
        Instance {
            binding: ContextBinding::Current,
            info: PluginInstance {
                id: "test".into(),
                plugin_id: manifest.id.clone(),
                view_id: "tab".into(),
                title: "Test".into(),
                window_label: "main".into(),
                ready: true,
                closed: false,
            },
            manifest,
            root: PathBuf::new(),
            entry: String::new(),
            permissions: vec![PluginPermission::ReadContext],
            events: VecDeque::new(),
            created: Instant::now(),
        }
    }
    #[test]
    fn denied_and_visual_context_never_leaks_paths() {
        let workspace = workspace().normalize().unwrap();
        let mut instance = instance();
        assert_eq!(
            value(&instance, Some(&workspace))
                .unwrap()
                .copy_operation
                .unwrap()
                .destination,
            "/target"
        );
        instance.permissions.clear();
        initialize(&mut instance, Some(&workspace));
        assert!(matches!(
            instance.events.pop_back().unwrap(),
            PluginEvent::Open {
                data: PluginViewOpen {
                    path: None,
                    context: None,
                    ..
                }
            }
        ));
        assert!(instance.authorize(&PluginRequest::Context).is_err());
        instance.permissions.push(PluginPermission::ReadContext);
        instance.binding = ContextBinding::Visual;
        assert!(value(&instance, Some(&workspace)).is_none());
    }
    #[test]
    fn navigation_tracks_the_bound_side_and_context_requests_are_fresh() {
        let broker = PluginBroker::default();
        let mut instance = instance();
        instance.binding = ContextBinding::Pane(PluginSide::Right);
        let state = Arc::new(Mutex::new(instance));
        broker
            .instances
            .lock()
            .unwrap()
            .insert("test".into(), state.clone());
        publish(&broker, workspace()).unwrap();
        state.lock().unwrap().events.clear();
        let mut next = workspace();
        next.left.path = "/changed-left".into();
        publish(&broker, next.clone()).unwrap();
        assert!(state.lock().unwrap().events.is_empty());
        next.right.path = "sftp://user@host:22/next".into();
        publish(&broker, next).unwrap();
        assert!(
            matches!(state.lock().unwrap().events.pop_front().unwrap(),PluginEvent::Path {data} if data.path.ends_with("/next"))
        );
        let value = broker
            .control("test", "main", PluginRequest::Context)
            .unwrap();
        assert_eq!(value["path"], "sftp://user@host:22/next");
    }
    #[test]
    fn malformed_and_oversized_snapshots_are_rejected() {
        let mut value = workspace();
        value.left.path = "relative".into();
        assert!(value.normalize().is_err());
        let mut value = workspace();
        value.left.selection = vec!["/long".repeat(1000); 100];
        assert!(value.normalize().is_err());
    }
}
