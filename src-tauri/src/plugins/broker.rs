//! Instance identities and grants are host-owned, never trusted from plugin data.
use super::{
    manifest::{PluginManifest, PluginPermission},
    protocol::{PluginEvent, PluginRequest},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use ts_rs::TS;

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginInstance {
    pub id: String,
    pub plugin_id: String,
    pub view_id: String,
    pub title: String,
    pub window_label: String,
    pub ready: bool,
    pub closed: bool,
}
pub struct Instance {
    pub subscriptions: Vec<super::protocol::PluginSubscription>,
    pub transfers: Vec<String>,
    pub binding: super::context::ContextBinding,
    pub info: PluginInstance,
    pub manifest: PluginManifest,
    pub root: PathBuf,
    pub entry: String,
    pub permissions: Vec<PluginPermission>,
    pub events: VecDeque<PluginEvent>,
    pub created: std::time::Instant,
}
impl Instance {
    pub fn stop(&mut self) {
        self.info.closed = true;
        self.events.clear();
        self.subscriptions.clear();
        for id in self.transfers.drain(..) {
            crate::commands::fs::transfer::registry::cancel_transfer(id);
        }
    }
    pub fn authorize(&self, request: &PluginRequest) -> Result<(), String> {
        if self.info.closed {
            return Err("Plugin instance is closed".into());
        }
        if !self.info.ready && self.created.elapsed().as_secs() > 10 {
            return Err("Plugin readiness timed out".into());
        }
        if request
            .permissions()
            .iter()
            .any(|p| !self.permissions.contains(p) || !self.manifest.permissions.contains(p))
        {
            return Err("Plugin permission denied".into());
        }
        Ok(())
    }
    pub fn push(&mut self, event: PluginEvent) {
        if matches!(event, PluginEvent::Context { .. }) {
            self.events
                .retain(|e| !matches!(e, PluginEvent::Context { .. }));
        }
        if let PluginEvent::Transfer { operation_id, .. } = &event {
            self.events.retain(|e|!matches!(e,PluginEvent::Transfer {operation_id:old,status,..} if old == operation_id && status == "progress"));
        }
        // Path changes supersede queued path changes for this instance.
        if matches!(event, PluginEvent::Path { .. }) {
            self.events
                .retain(|e| !matches!(e, PluginEvent::Path { .. }));
        }
        if self.events.len() == 128 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}
#[derive(Default)]
pub struct PluginBroker {
    pub management: Mutex<()>,
    pub limit_override: Mutex<Option<u32>>,
    pub pending: Mutex<BTreeMap<String, super::transfers::PendingTransfer>>,
    pub ui: Mutex<VecDeque<super::actions::PluginUiEvent>>,
    pub workspace: Mutex<Option<super::context::PluginWorkspace>>,
    pub native_processes: Mutex<BTreeMap<String, u32>>,
    pub instances: Mutex<BTreeMap<String, Arc<Mutex<Instance>>>>,
    pub grants: Mutex<BTreeMap<String, Vec<PluginPermission>>>,
}
impl PluginBroker {
    pub fn instance(&self, id: &str, caller: &str) -> Result<Arc<Mutex<Instance>>, String> {
        let instances = self
            .instances
            .lock()
            .map_err(|_| "Plugin registry unavailable")?;
        let instance = instances.get(id).ok_or("Unknown plugin instance")?.clone();
        let state = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        if caller != "main" && caller != state.info.window_label {
            return Err("Plugin instance belongs to a different window".into());
        }
        if state.info.closed {
            return Err("Plugin instance is closed".into());
        }
        drop(state);
        Ok(instance)
    }
    pub fn close(&self, id: &str) {
        if let Ok(mut instances) = self.instances.lock() {
            if let Some(instance) = instances.remove(id) {
                if let Ok(mut instance) = instance.lock() {
                    instance.stop();
                    instance.events.clear();
                }
            }
        }
    }
    pub fn close_window(&self, label: &str) {
        if let Ok(mut instances) = self.instances.lock() {
            instances.retain(|_, instance| {
                if let Ok(mut instance) = instance.lock() {
                    if instance.info.window_label == label {
                        instance.stop();
                        return false;
                    }
                }
                true
            });
        }
    }
    pub fn control(
        &self,
        id: &str,
        caller: &str,
        request: PluginRequest,
    ) -> Result<serde_json::Value, String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "Plugin context unavailable")?
            .clone();
        let instance = self.instance(id, caller)?;
        let mut instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        instance.authorize(&request)?;
        match request {
            PluginRequest::Subscribe { event } => {
                if !instance.subscriptions.contains(&event) {
                    instance.subscriptions.push(event);
                }
                Ok(serde_json::Value::Null)
            }
            PluginRequest::Unsubscribe { event } => {
                instance.subscriptions.retain(|e| *e != event);
                Ok(serde_json::Value::Null)
            }
            PluginRequest::Context => {
                serde_json::to_value(super::context::value(&instance, workspace.as_ref()))
                    .map_err(|e| e.to_string())
            }
            PluginRequest::Ping => Ok(serde_json::json!("pong")),
            PluginRequest::Ready => {
                instance.info.ready = true;
                Ok(serde_json::Value::Null)
            }
            PluginRequest::Title { title } if !title.is_empty() && title.len() <= 256 => {
                instance.info.title = title;
                Ok(serde_json::Value::Null)
            }
            PluginRequest::Close => {
                instance.stop();
                Ok(serde_json::Value::Null)
            }
            _ => Err("Broker action is not available".into()),
        }
    }
}

impl Drop for PluginBroker {
    fn drop(&mut self) {
        if let Ok(instances) = self.instances.lock() {
            for instance in instances.values() {
                if let Ok(mut instance) = instance.lock() {
                    instance.stop();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PluginBroker {
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
            "id":"org.test.plugin", "name":"Test", "version":"1.0.0", "description":"Test", "author":"Test", "apiVersion":1,
            "runtime":"js", "entry":"index.html", "icon":"🌈", "permissions":["read:context"]
        })).unwrap();
        let broker = PluginBroker::default();
        let instance = Instance {
            binding: Default::default(),
            subscriptions: vec![],
            transfers: vec![],
            info: PluginInstance {
                id: "session".into(),
                plugin_id: manifest.id.clone(),
                view_id: "window".into(),
                title: "Test".into(),
                window_label: "plugin-session".into(),
                ready: false,
                closed: false,
            },
            manifest,
            root: PathBuf::new(),
            entry: "index.html".into(),
            permissions: vec![],
            events: VecDeque::new(),
            created: std::time::Instant::now(),
        };
        broker
            .instances
            .lock()
            .unwrap()
            .insert("session".into(), Arc::new(Mutex::new(instance)));
        broker
    }
    #[test]
    fn requests_cannot_escalate_declared_permissions() {
        let broker = fixture();
        assert!(broker
            .control("session", "plugin-other", PluginRequest::Ping)
            .is_err());
        assert_eq!(
            broker
                .control("session", "plugin-session", PluginRequest::Ping)
                .unwrap(),
            "pong"
        );
        assert!(broker
            .control("session", "main", PluginRequest::Context)
            .is_err());
        assert!(broker
            .control("session", "main", PluginRequest::Ready)
            .is_ok());
        broker.close_window("plugin-session");
        assert!(broker
            .control("session", "main", PluginRequest::Ping)
            .is_err());
    }
    #[test]
    fn subscriptions_and_transfer_requests_require_grants_and_stop_on_close() {
        use super::super::{
            context::PluginWorkspace,
            protocol::{PluginContext, PluginSide, PluginSubscription},
            transfers,
        };
        let broker = fixture();
        let instance = broker.instance("session", "main").unwrap();
        let request = PluginRequest::Subscribe {
            event: PluginSubscription::Transfer,
        };
        assert!(broker.control("session", "main", request.clone()).is_err());
        {
            let mut state = instance.lock().unwrap();
            state
                .manifest
                .permissions
                .extend([PluginPermission::Transfer, PluginPermission::Copy]);
            state.permissions = vec![PluginPermission::Transfer, PluginPermission::Copy];
        }
        broker.control("session", "main", request).unwrap();
        super::super::events::transfer(&broker, "operation", "progress", 10.0);
        super::super::events::transfer(&broker, "operation", "progress", 20.0);
        assert_eq!(instance.lock().unwrap().events.len(), 1);
        let context = PluginContext {
            path: "/".into(),
            folder_path: "/".into(),
            file_path: None,
            selection: vec![],
            side: PluginSide::Left,
            copy_limit: 42,
            copy_operation: None,
        };
        *broker.workspace.lock().unwrap() = Some(PluginWorkspace {
            active: PluginSide::Left,
            left: context.clone(),
            right: context,
        });
        let transfer = transfers::enqueue(
            &broker,
            "session",
            "main",
            transfers::PluginTransferKind::Copy,
            vec!["/source".into()],
            "/target".into(),
        )
        .unwrap();
        assert_eq!(transfer.copy_limit, 42);
        assert_eq!(transfers::pending(&broker).unwrap().len(), 1);
        assert!(transfers::enqueue(
            &broker,
            "session",
            "main",
            transfers::PluginTransferKind::Move,
            vec!["/source".into()],
            "/target".into()
        )
        .is_err());
        broker.close("session");
        assert!(transfers::pending(&broker).unwrap().is_empty());
        assert!(instance.lock().unwrap().subscriptions.is_empty());
    }
}
