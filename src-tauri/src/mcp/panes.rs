//! Read-only, explicitly typed window context. Credentials and arbitrary UI
//! state cannot enter a resource through this contract.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpPane {
    pub path: String,
    pub selection: Vec<String>,
    pub sort: String,
    pub filter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct McpPanes {
    pub active: String,
    pub left: McpPane,
    pub right: McpPane,
}

fn state() -> &'static Mutex<Option<Value>> {
    static STATE: OnceLock<Mutex<Option<Value>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn publish(value: Value) {
    *state().lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
}

#[cfg(test)]
pub fn clear() {
    *state().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn side(side: &str) -> Option<Value> {
    state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()?
        .get(side)
        .cloned()
}

pub fn active() -> Option<Value> {
    let state = state().lock().unwrap_or_else(|e| e.into_inner());
    let value = state.as_ref()?;
    value.get(value.get("active")?.as_str()?).cloned()
}

pub fn connections() -> Value {
    json!(crate::net::session::connection_states()
        .iter()
        .filter(|c| c.connected)
        .filter_map(|c| crate::vfs::parse_address(&c.address)
            .ok()
            .map(|a| json!({ "name": c.name, "scheme": a.scheme().as_str() })))
        .collect::<Vec<_>>())
}

pub fn effective_config() -> super::McpConfig {
    let mut config = super::config();
    if config.roots_follow_panes {
        for side_name in ["left", "right"] {
            if let Some(value) = side(side_name) {
                if let Some(path) = value.get("path").and_then(Value::as_str) {
                    config.roots.push(path.to_string());
                }
            }
        }
    }
    if config.roots_follow_remotes {
        config.roots.extend(
            crate::net::session::connection_states()
                .into_iter()
                .filter(|connection| connection.connected)
                .map(|connection| connection.address),
        );
    }
    config
}
