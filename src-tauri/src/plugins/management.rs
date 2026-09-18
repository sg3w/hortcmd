//! Explicit grants are bound to the installed package contents and stored locally.
use super::{
    broker::PluginBroker,
    manifest::{valid_id, PluginPermission},
    package,
    registry::{self, PluginRecord},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Clone, Serialize, Deserialize)]
struct Approval {
    fingerprint: String,
    permissions: Vec<PluginPermission>,
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    approvals: BTreeMap<String, Approval>,
}
fn load(root: &Path) -> Result<State, String> {
    match fs::read(root.join(".state.json")) {
        Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid plugin approval state: {e}")),
        Ok(_) => Err("Plugin approval state exceeds 1 MiB".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(root: &Path, state: &State) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let temporary = package::Staging::new(root)?;
    let path = temporary.path().join("state.json");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut file, state).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    fs::rename(path, root.join(".state.json")).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn stop(broker: &PluginBroker, id: &str) {
    if let Ok(mut grants) = broker.grants.lock() {
        grants.remove(id);
    }
    if let Ok(mut instances) = broker.instances.lock() {
        instances.retain(|_, instance| {
            if let Ok(mut instance) = instance.lock() {
                if instance.info.plugin_id == id {
                    instance.stop();
                    return false;
                }
            }
            true
        });
    }
}
pub fn records(broker: &PluginBroker, root: &Path) -> Result<Vec<PluginRecord>, String> {
    let _guard = broker
        .management
        .lock()
        .map_err(|_| "Plugin management unavailable")?;
    let state = match load(root) {
        Ok(state) => state,
        Err(error) => {
            let ids: Vec<_> = broker
                .grants
                .lock()
                .map_err(|_| "Plugin grants unavailable")?
                .keys()
                .cloned()
                .collect();
            for id in ids {
                stop(broker, &id);
            }
            return Err(error);
        }
    };
    let mut records = registry::discover(root)?;
    let mut granted = BTreeMap::new();
    for record in &mut records {
        if let Some(manifest) = &record.manifest {
            match package::fingerprint(&root.join(&record.id)) {
                Ok(fingerprint) => {
                    if let Some(approval) = state.approvals.get(&record.id).filter(|a| {
                        a.fingerprint == fingerprint && a.permissions == manifest.permissions
                    }) {
                        record.active = true;
                        record.granted = approval.permissions.clone();
                        granted.insert(record.id.clone(), record.granted.clone());
                    }
                    record.fingerprint = Some(fingerprint);
                }
                Err(error) => record.error = Some(error),
            }
        }
    }
    let old: Vec<_> = broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?
        .keys()
        .cloned()
        .collect();
    for id in old {
        if !granted.contains_key(&id) {
            stop(broker, &id);
        }
    }
    *broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")? = granted;
    Ok(records)
}
pub fn activate(
    broker: &PluginBroker,
    root: &Path,
    id: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let _guard = broker
        .management
        .lock()
        .map_err(|_| "Plugin management unavailable")?;
    if !valid_id(id) {
        return Err("Invalid plugin id".into());
    }
    let manifest = registry::read_manifest(&root.join(id))?;
    if manifest.id != id || package::fingerprint(&root.join(id))? != fingerprint {
        return Err("Plugin changed; refresh and review its permissions again".into());
    }
    let mut state = load(root)?;
    state.approvals.insert(
        id.into(),
        Approval {
            fingerprint: fingerprint.into(),
            permissions: manifest.permissions.clone(),
        },
    );
    save(root, &state)?;
    broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?
        .insert(id.into(), manifest.permissions);
    Ok(())
}
pub fn deactivate(
    broker: &PluginBroker,
    root: &Path,
    id: &str,
    remove: bool,
) -> Result<(), String> {
    let _guard = broker
        .management
        .lock()
        .map_err(|_| "Plugin management unavailable")?;
    if !valid_id(id) {
        return Err("Invalid plugin id".into());
    }
    let mut state = load(root)?;
    state.approvals.remove(id);
    save(root, &state)?;
    stop(broker, id);
    if remove {
        let path = root.join(id);
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn version(value: &str) -> Result<Vec<u64>, String> {
    value
        .split('.')
        .map(|s| s.parse().map_err(|_| "Invalid numeric version".into()))
        .collect()
}
pub fn install(
    broker: &PluginBroker,
    root: &Path,
    source: &Path,
    update: Option<&str>,
) -> Result<String, String> {
    let _guard = broker
        .management
        .lock()
        .map_err(|_| "Plugin management unavailable")?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let staging = package::Staging::new(root)?;
    package::stage(source, staging.path())?;
    let manifest = registry::read_manifest(staging.path())?;
    let id = &manifest.id;
    let destination = root.join(id);
    if let Some(expected) = update {
        if expected != id {
            return Err("Update must have the same plugin id".into());
        }
        let old = registry::read_manifest(&destination)?;
        if version(&manifest.version)? <= version(&old.version)? {
            return Err("Update requires a newer version".into());
        }
    } else if destination.exists() {
        return Err("Plugin already installed; choose Update".into());
    }
    let mut state = load(root)?;
    state.approvals.remove(id);
    save(root, &state)?;
    stop(broker, id);
    let backup = root.join(format!(".backup-{}", crate::net::unique_id()));
    if update.is_some() {
        fs::rename(&destination, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(error) = fs::rename(staging.path(), &destination) {
        if update.is_some() {
            fs::rename(&backup, &destination).map_err(|rollback| {
                format!("Install failed: {error}; rollback failed: {rollback}")
            })?;
        }
        return Err(error.to_string());
    }
    if update.is_some() {
        fs::remove_dir_all(backup).map_err(|e| e.to_string())?;
    }
    Ok(id.clone())
}
#[cfg(test)]
mod tests;
