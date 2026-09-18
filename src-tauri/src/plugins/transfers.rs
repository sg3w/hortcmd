//! Plugin transfers require the same user review and recovery journal as core UI
//! transfers. The plugin receives an operation ID, never an approval bypass.
use super::{broker::PluginBroker, protocol::PluginRequest};
use crate::commands::fs::transfer::recovery::{
    commands as recovery,
    types::{RecoveryKind, RecoverySettings},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::Manager;
use ts_rs::TS;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginTransferKind {
    Copy,
    Move,
}
#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginTransfer {
    pub id: String,
    pub instance_id: String,
    pub plugin_name: String,
    pub kind: PluginTransferKind,
    pub sources: Vec<String>,
    pub destination: String,
    pub copy_limit: u32,
}
pub struct PendingTransfer {
    pub claimed: bool,
    pub transfer: PluginTransfer,
    pub created: Instant,
}
impl PluginTransfer {
    pub fn request(&self) -> PluginRequest {
        match self.kind {
            PluginTransferKind::Copy => PluginRequest::Copy {
                sources: self.sources.clone(),
                destination: self.destination.clone(),
            },
            PluginTransferKind::Move => PluginRequest::Move {
                sources: self.sources.clone(),
                destination: self.destination.clone(),
            },
        }
    }
}
pub fn enqueue(
    broker: &PluginBroker,
    instance_id: &str,
    caller: &str,
    kind: PluginTransferKind,
    sources: Vec<String>,
    destination: String,
) -> Result<PluginTransfer, String> {
    if sources.is_empty() || sources.len() > 1000 {
        return Err("A transfer requires 1 to 1000 sources".into());
    }
    for path in sources.iter().chain(std::iter::once(&destination)) {
        super::actions::validate_path(path)?;
    }
    let copy_limit = broker
        .workspace
        .lock()
        .map_err(|_| "Plugin context unavailable")?
        .as_ref()
        .ok_or("No copy-limit snapshot available")?
        .left
        .copy_limit;
    let instance = broker.instance(instance_id, caller)?;
    let state = instance.lock().map_err(|_| "Plugin instance unavailable")?;
    let transfer = PluginTransfer {
        id: format!("plugin-{}", crate::net::unique_id()),
        instance_id: instance_id.to_string(),
        plugin_name: state.manifest.name.clone(),
        kind,
        sources,
        destination,
        copy_limit,
    };
    state.authorize(&transfer.request())?;
    let mut pending = broker
        .pending
        .lock()
        .map_err(|_| "Plugin transfer queue unavailable")?;
    if pending.len() >= 32 {
        return Err("Too many pending plugin transfers".into());
    }
    pending.insert(
        transfer.id.clone(),
        PendingTransfer {
            claimed: false,
            transfer: transfer.clone(),
            created: Instant::now(),
        },
    );
    Ok(transfer)
}
pub fn pending(broker: &PluginBroker) -> Result<Vec<PluginTransfer>, String> {
    let active: Vec<_> = broker
        .instances
        .lock()
        .map_err(|_| "Plugin registry unavailable")?
        .iter()
        .filter(|(_, s)| s.lock().is_ok_and(|s| !s.info.closed))
        .map(|(id, _)| id.clone())
        .collect();
    let mut pending = broker
        .pending
        .lock()
        .map_err(|_| "Plugin transfer queue unavailable")?;
    pending.retain(|_, p| {
        p.created.elapsed() < Duration::from_secs(1800) && active.contains(&p.transfer.instance_id)
    });
    let mut values: Vec<_> = pending.values().filter(|p| !p.claimed).collect();
    values.sort_by_key(|p| p.created);
    Ok(values.into_iter().map(|p| p.transfer.clone()).collect())
}
pub fn cancel(broker: &PluginBroker, id: &str) -> Result<(), String> {
    broker
        .pending
        .lock()
        .map_err(|_| "Plugin transfer queue unavailable")?
        .remove(id);
    crate::commands::fs::transfer::registry::cancel_transfer(id.into());
    super::events::transfer(broker, id, "cancelled", 0.0);
    Ok(())
}
pub async fn start(
    app: &tauri::AppHandle,
    id: &str,
    settings: RecoverySettings,
    review: String,
) -> Result<(), String> {
    let broker = app.state::<PluginBroker>();
    let transfer = {
        let mut queue = broker
            .pending
            .lock()
            .map_err(|_| "Plugin transfer queue unavailable")?;
        let pending = queue
            .get_mut(id)
            .ok_or("Transfer request expired or was cancelled")?;
        if pending.claimed {
            return Err("Transfer is already starting".into());
        }
        pending.claimed = true;
        pending.transfer.clone()
    };
    let result = start_claimed(app, &transfer, settings, review).await;
    if result.is_err() {
        broker
            .pending
            .lock()
            .map_err(|_| "Plugin transfer queue unavailable")?
            .remove(id);
        let _ = recovery::recovery_discard(id.to_string()).await;
        super::events::transfer(&broker, id, "failed", 0.0);
    }
    result
}
async fn start_claimed(
    app: &tauri::AppHandle,
    transfer: &PluginTransfer,
    settings: RecoverySettings,
    review: String,
) -> Result<(), String> {
    let broker = app.state::<PluginBroker>();
    let id = &transfer.id;
    let instance = broker.instance(&transfer.instance_id, "main")?;
    instance
        .lock()
        .map_err(|_| "Plugin instance unavailable")?
        .authorize(&transfer.request())?;
    if settings.limit != u64::from(transfer.copy_limit) * 1024 {
        return Err("Copy limit changed; review this transfer again".into());
    }
    if review.is_empty() {
        return Err("A reviewed preflight plan is required".into());
    }
    let kind = match transfer.kind {
        PluginTransferKind::Copy => RecoveryKind::Copy,
        PluginTransferKind::Move => RecoveryKind::Move,
    };
    let check_transfer = transfer.clone();
    let check_settings = settings.clone();
    let check_kind = kind.clone();
    let check_review = review.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let plan = crate::commands::fs::transfer::preflight::build(
            check_kind,
            &check_transfer.sources,
            &check_transfer.destination,
            check_settings,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .map_err(|e| e.to_string())?;
        plan.ensure_executable().map_err(|e| e.to_string())?;
        plan.matches_review(&check_review)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    recovery::recovery_queue_transfer(
        id.to_string(),
        kind,
        transfer.sources.clone(),
        transfer.destination.clone(),
        settings.clone(),
        Some(review),
    )
    .await
    .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut instance = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        instance.authorize(&transfer.request())?;
        let mut queue = broker
            .pending
            .lock()
            .map_err(|_| "Plugin transfer queue unavailable")?;
        if !queue.contains_key(id) {
            return Err("Transfer was cancelled".into());
        }
        instance.transfers.push(id.clone());
        let launch = match transfer.kind {
            PluginTransferKind::Copy => crate::commands::fs::file::copy_entries,
            PluginTransferKind::Move => crate::commands::fs::file::move_entries,
        };
        launch(
            app.clone(),
            id.to_string(),
            transfer.sources.clone(),
            transfer.destination.clone(),
            settings.limit,
            settings.verify,
            settings.buf_kb,
            settings.threads,
            settings.safe,
            settings.check_space,
        );
        queue.remove(id);
        Ok(())
    })();
    if result.is_err() {
        let _ = recovery::recovery_discard(id.to_string()).await;
    }
    result
}
