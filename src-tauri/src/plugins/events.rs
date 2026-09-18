use super::{
    broker::PluginBroker,
    manifest::PluginPermission,
    protocol::{PluginEvent, PluginSubscription},
};
use serde_json::Value;
use tauri::{Listener, Manager};
pub fn transfer(broker: &PluginBroker, id: &str, status: &str, bytes: f64) {
    if let Ok(instances) = broker.instances.lock() {
        for instance in instances.values() {
            if let Ok(mut instance) = instance.lock() {
                if status != "progress" {
                    instance.transfers.retain(|operation| operation != id);
                }
                if !instance.info.closed
                    && instance.permissions.contains(&PluginPermission::Transfer)
                    && instance
                        .subscriptions
                        .contains(&PluginSubscription::Transfer)
                {
                    instance.push(PluginEvent::Transfer {
                        operation_id: id.into(),
                        status: status.into(),
                        bytes,
                    });
                }
            }
        }
    }
}
pub fn attach(app: &tauri::AppHandle) {
    for name in ["fs-progress", "fs-done"] {
        let handle = app.clone();
        app.listen(name, move |event| {
            let Ok(value) = serde_json::from_str::<Value>(event.payload()) else {
                return;
            };
            let Some(id) = value["id"].as_str() else {
                return;
            };
            let status = if name == "fs-progress" {
                "progress"
            } else if value["cancelled"] == true {
                "cancelled"
            } else if value["errors"].as_array().is_some_and(|e| !e.is_empty()) {
                "failed"
            } else {
                "completed"
            };
            transfer(
                &handle.state::<PluginBroker>(),
                id,
                status,
                value["bytes_done"].as_f64().unwrap_or(0.0),
            );
        });
    }
}
