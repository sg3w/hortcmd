// ============================================================
// Connection profiles + live session registry (ticket 021).
//
// Profiles (no secrets) are persisted as JSON in the app config dir;
// secrets live in the OS keychain via `CredentialStore`. The registry
// owns the live SSH/SFTP sessions (one per `user@host:port`), which
// the `sftp` VFS provider drives through a shared Tokio runtime so the
// async russh client fits the synchronous `Provider` trait.
// ============================================================

use super::credentials::{CredentialStore, KeychainStore};
use crate::vfs::Provider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use ts_rs::TS;

// ---------- Types ----------

/// A saved connection profile. Never contains the secret itself —
/// passwords/passphrases live in the `CredentialStore` under `id`.
#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Protocol scheme, e.g. `"sftp"` (others reserved).
    pub protocol: String,
    pub host: String,
    #[ts(type = "number")]
    pub port: u16,
    pub user: String,
    /// `"password"` | `"agent"` | `"key"` | `"anonymous"`.
    pub auth: String,
    /// Path to a private key when `auth == "key"`.
    pub key_path: Option<String>,
    /// Directory to open on connect.
    pub start_path: String,
}

impl Profile {
    /// Connection key `user@host:port` — the identity shared by all
    /// addresses pointing at this server. A scheme prefix on the host
    /// (WebDAV allows `http://host`) is stripped so the key — and the
    /// addresses built from it — stay well-formed.
    pub fn connection_key(&self) -> String {
        let host = self
            .host
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(&self.host)
            .trim_end_matches('/');
        format!("{}@{}:{}", self.user, host, self.port)
    }

    /// VFS address scheme for this profile. FTPS is not a separate
    /// scheme — it is the `ftp` provider with TLS enabled.
    pub fn scheme(&self) -> &str {
        match self.protocol.as_str() {
            "ftps" => "ftp",
            other => other,
        }
    }

    /// Identity of the store this profile connects to — must match the
    /// provider's `store_id()` so the registry lookup finds it.
    pub fn store_id(&self) -> String {
        format!("{}:{}", self.scheme(), self.connection_key())
    }

    /// Navigable VFS address for the profile's start path.
    pub fn address(&self) -> String {
        let start = if self.start_path.is_empty() {
            "/"
        } else {
            &self.start_path
        };
        format!(
            "{}://{}@{}:{}{}",
            self.scheme(),
            self.user,
            self.host,
            self.port,
            start
        )
    }
}

/// Live status of a connection (for the location selector).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ConnectionStatus {
    pub id: String,
    pub key: String,
    pub name: String,
    pub connected: bool,
    /// `sftp://user@host:port/start` — navigate here to browse.
    pub address: String,
}

// ---------- Persistence ----------

fn config_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Kein Konfigurationsverzeichnis gefunden".to_string())?
        .join("hortcmd");
    Ok(dir.join("connections.json"))
}

fn load_profiles() -> Vec<Profile> {
    let Ok(path) = config_path() else {
        return Vec::new();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn store_profiles(profiles: &[Profile]) -> Result<(), String> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(profiles).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

// ---------- Registry ----------

/// Holds saved profiles and the live connections. Connections are keyed
/// by their provider's `store_id()` (`"<scheme>:<user@host:port>"`), so
/// one registry serves every protocol.
struct Registry {
    profiles: Vec<Profile>,
    live: HashMap<String, Arc<dyn Provider>>,
    creds: Box<dyn CredentialStore>,
}

fn registry() -> &'static Mutex<Registry> {
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(Registry {
            profiles: load_profiles(),
            live: HashMap::new(),
            creds: Box::new(KeychainStore::new()),
        })
    })
}

/// Shared multi-thread Tokio runtime driving the async protocol clients
/// (SFTP via russh, WebDAV via reqwest). Static, so sessions and their
/// spawned tasks stay alive; the synchronous provider/transfer threads
/// reach it via `block_on`.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Tokio-Runtime konnte nicht gestartet werden")
    })
}

// ---------- Public API used by the VFS resolver ----------

/// Resolves an active remote connection into its VFS provider.
/// `scheme` is e.g. `"sftp"`, `key` the address' `user@host:port`.
/// Returns a clear error when nothing is connected for that address.
pub fn remote_provider(scheme: &str, key: &str) -> Result<Arc<dyn Provider>, String> {
    let store_id = format!("{}:{}", scheme, key);
    let reg = registry().lock().unwrap();
    match reg.live.get(&store_id) {
        Some(p) => Ok(p.clone()),
        None => Err(format!(
            "Keine aktive {}-Verbindung für {} — bitte zuerst verbinden",
            scheme.to_uppercase(),
            key
        )),
    }
}

// ---------- Commands ----------

/// Lists saved connection profiles.
#[tauri::command]
pub fn net_list_profiles() -> Vec<Profile> {
    registry().lock().unwrap().profiles.clone()
}

/// Creates or updates a profile. When `secret` is `Some`, it is stored
/// in the OS keychain under the profile id (never in the JSON).
#[tauri::command]
pub fn net_save_profile(profile: Profile, secret: Option<String>) -> Result<(), String> {
    let mut reg = registry().lock().unwrap();
    if let Some(s) = secret {
        reg.creds.set(&profile.id, &s)?;
    }
    match reg.profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => reg.profiles.push(profile),
    }
    store_profiles(&reg.profiles)
}

/// Deletes a profile and its stored secret (disconnecting if live).
#[tauri::command]
pub fn net_delete_profile(id: String) -> Result<(), String> {
    let mut reg = registry().lock().unwrap();
    let mut closed = None;
    if let Some(pos) = reg.profiles.iter().position(|p| p.id == id) {
        let store_id = reg.profiles[pos].store_id();
        reg.live.remove(&store_id);
        reg.profiles.remove(pos);
        closed = Some(store_id);
    }
    let _ = reg.creds.delete(&id);
    let result = store_profiles(&reg.profiles);
    drop(reg);
    if let Some(store_id) = closed {
        super::terminal::close_for_store(&store_id);
    }
    result
}

/// Opens a connection for a profile and returns its status. Reconnects
/// (replaces) an existing session for the same key.
#[tauri::command]
pub fn net_connect(id: String) -> Result<ConnectionStatus, String> {
    // Collect what we need under the lock, then connect without holding it.
    let (profile, secret) = {
        let reg = registry().lock().unwrap();
        let profile = reg
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("Profil {} nicht gefunden", id))?;
        let secret = reg.creds.get(&id)?;
        (profile, secret)
    };

    // Dispatch to the protocol's provider. Each `connect` returns a ready
    // provider bound to the live connection.
    let provider: Arc<dyn Provider> = match profile.protocol.as_str() {
        "sftp" => crate::vfs::sftp::connect(&profile, secret.as_deref())?,
        "ftp" | "ftps" => crate::vfs::ftp::connect(&profile, secret.as_deref())?,
        "webdav" => crate::vfs::webdav::connect(&profile, secret.as_deref())?,
        "smb" => crate::vfs::smb::connect(&profile, secret.as_deref())?,
        other => {
            return Err(format!("Protokoll '{}' wird noch nicht unterstützt", other));
        }
    };

    let key = profile.connection_key();
    registry()
        .lock()
        .unwrap()
        .live
        .insert(provider.store_id(), provider);

    Ok(ConnectionStatus {
        id: profile.id.clone(),
        key,
        name: profile.name.clone(),
        connected: true,
        address: profile.address(),
    })
}

/// Disconnects a live connection (by profile id). Profile stays saved.
/// Open shell sessions on that connection are closed with it (026), so
/// no channel outlives the connection it rides on.
#[tauri::command]
pub fn net_disconnect(id: String) -> Result<(), String> {
    let mut reg = registry().lock().unwrap();
    let store_id = reg
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|profile| profile.store_id());
    if let Some(store_id) = &store_id {
        reg.live.remove(store_id);
    }
    drop(reg);
    if let Some(store_id) = store_id {
        super::terminal::close_for_store(&store_id);
    }
    Ok(())
}

/// Lists saved profiles with their live-connection status.
#[tauri::command]
pub fn net_list_connections() -> Vec<ConnectionStatus> {
    let reg = registry().lock().unwrap();
    reg.profiles
        .iter()
        .map(|p| ConnectionStatus {
            id: p.id.clone(),
            key: p.connection_key(),
            name: p.name.clone(),
            connected: reg.live.contains_key(&p.store_id()),
            address: p.address(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile {
            id: "p1".into(),
            name: "Test".into(),
            protocol: "sftp".into(),
            host: "example.com".into(),
            port: 2222,
            user: "alice".into(),
            auth: "password".into(),
            key_path: None,
            start_path: "/srv".into(),
        }
    }

    #[test]
    fn connection_key_and_address() {
        let p = profile();
        assert_eq!(p.connection_key(), "alice@example.com:2222");
        assert_eq!(p.address(), "sftp://alice@example.com:2222/srv");
    }

    #[test]
    fn empty_start_path_defaults_to_root() {
        let mut p = profile();
        p.start_path = String::new();
        assert_eq!(p.address(), "sftp://alice@example.com:2222/");
    }

    #[test]
    fn profiles_json_roundtrips() {
        let list = vec![profile()];
        let json = serde_json::to_vec(&list).unwrap();
        let back: Vec<Profile> = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].connection_key(), "alice@example.com:2222");
    }
}
