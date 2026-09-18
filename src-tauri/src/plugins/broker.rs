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
    pub info: PluginInstance,
    pub manifest: PluginManifest,
    pub root: PathBuf,
    pub entry: String,
    pub permissions: Vec<PluginPermission>,
    pub events: VecDeque<PluginEvent>,
    pub created: std::time::Instant,
}
impl Instance {
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
        if self.events.len() == 128 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}
#[derive(Default)]
pub struct PluginBroker {
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
                    instance.info.closed = true;
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
                        instance.info.closed = true;
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
        let instance = self.instance(id, caller)?;
        let mut instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        instance.authorize(&request)?;
        match request {
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
                instance.info.closed = true;
                Ok(serde_json::Value::Null)
            }
            _ => Err("Broker action is not available".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requests_cannot_escalate_declared_permissions() {
        let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
            "id":"org.test.plugin", "name":"Test", "version":"1.0.0", "description":"Test", "author":"Test", "apiVersion":1,
            "runtime":"js", "entry":"index.html", "icon":"🌈", "permissions":["read:context"]
        })).unwrap();
        let broker = PluginBroker::default();
        let instance = Instance {
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
}
