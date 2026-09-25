//! Instance-owned, pull-streamed storage scans. Each request does a bounded slice
//! of work on the existing blocking broker worker, never the UI or broker lock.
mod scan;
pub mod types;
use super::{
    broker::{Instance, PluginBroker},
    protocol::PluginRequest,
};
use scan::Scan;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use types::UsageOptions;
#[derive(Default)]
pub struct Handle {
    cancel: AtomicBool,
    scan: Mutex<Option<Scan>>,
}
impl Handle {
    pub fn close(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut scan) = self.scan.try_lock() {
            scan.take();
        }
    }
}
fn approved(
    instance: &Instance,
    workspace: Option<&super::context::PluginWorkspace>,
    options: &UsageOptions,
) -> Result<(), String> {
    options.validate()?;
    let context = super::context::value(instance, workspace).ok_or("No approved folder context")?;
    let candidate =
        crate::vfs::location::VfsLocation::parse(&options.root).map_err(|e| e.to_string())?;
    if candidate.scheme() != crate::vfs::Scheme::Local {
        return Err("This provider does not advertise recursive size and metadata support".into());
    }
    let root = std::path::PathBuf::from(candidate.path())
        .canonicalize()
        .map_err(|_| "Cannot resolve scan root")?;
    let parent = crate::vfs::location::VfsLocation::parse(&context.folder_path)
        .map_err(|e| e.to_string())?;
    if parent.scheme() != crate::vfs::Scheme::Local {
        return Err("Select a supported local folder in the commander first".into());
    }
    let parent = std::path::PathBuf::from(parent.path())
        .canonicalize()
        .map_err(|_| "Cannot resolve approved folder")?;
    let explicit = match &instance.binding {
        super::context::ContextBinding::Preview { path, .. } => {
            std::path::PathBuf::from(path).canonicalize().ok()
        }
        _ => None,
    };
    if !root.starts_with(parent) && !explicit.is_some_and(|path| root.starts_with(path)) {
        return Err("Scan root is outside the approved pane folder".into());
    }
    Ok(())
}
pub fn execute(
    broker: &PluginBroker,
    id: &str,
    caller: &str,
    request: &PluginRequest,
) -> Result<serde_json::Value, String> {
    let workspace = broker
        .workspace
        .lock()
        .map_err(|_| "Context unavailable")?
        .clone();
    let instance = broker.instance(id, caller)?;
    let handle = {
        let state = instance.lock().map_err(|_| "Plugin instance unavailable")?;
        state.authorize(request)?;
        if let PluginRequest::UsageStart { options } = request {
            approved(&state, workspace.as_ref(), options)?;
        }
        state.usage.clone()
    };
    let mut guard = handle
        .scan
        .try_lock()
        .map_err(|_| "A scan request is already running")?;
    if let PluginRequest::UsageStart { options } = request {
        guard.take();
        handle.cancel.store(false, Ordering::Relaxed);
        *guard = Some(Scan::new(options.clone())?);
    }
    let scan = guard.as_mut().ok_or("No storage scan in this instance")?;
    match request {
        PluginRequest::UsageStep => {
            if let Err(error) = scan.step(&handle.cancel, &|| {
                instance
                    .lock()
                    .map_err(|_| "Plugin instance unavailable")?
                    .authorize(request)
            }) {
                guard.take();
                return Err(error);
            }
        }
        PluginRequest::UsageControl { action } => match action.as_str() {
            "pause" if scan.status == "running" => scan.status = "paused".into(),
            "resume" if scan.status == "paused" => scan.status = "running".into(),
            "cancel" => scan.stop("cancelled"),
            _ => return Err("Invalid scan control for the current state".into()),
        },
        PluginRequest::UsageStart { .. } => {}
        _ => return Err("Not a usage request".into()),
    }
    let authorization = instance
        .lock()
        .map_err(|_| "Plugin instance unavailable")?
        .authorize(request);
    if let Err(error) = authorization {
        guard.take();
        return Err(error);
    }
    if handle.cancel.load(Ordering::Relaxed) {
        guard.take();
        return Err("Scan instance closed".into());
    }
    serde_json::to_value(scan.batch()).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests;
