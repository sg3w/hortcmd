// ============================================================
// Connection profiles + live session registry (ticket 021).
//
// Profiles (no secrets) are persisted as JSON in the app config dir;
// secrets live in the configured `CredentialStore` (OS keychain by
// default, optionally a KeePass file — ticket 027). The registry
// owns the live SSH/SFTP sessions (one per `user@host:port`), which
// the `sftp` VFS provider drives through a shared Tokio runtime so the
// async russh client fits the synchronous `Provider` trait.
// ============================================================

use super::credentials::{self, CredentialStore};
use super::hostkey::{self, ConnectError, ForgetResult, HostId, HostKeyChallenge};
use super::lifecycle::{self, Attempt, ConnFailure, FailureKind, Phase, Start, OVERALL_TIMEOUT};
use crate::errors::{AppError, AppResult};
use crate::vfs::Provider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

// ---------- Types ----------

/// A saved connection profile. Never contains the secret itself —
/// passwords/passphrases live in the `CredentialStore` under `id`.
#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Profile {
    #[serde(default)]
    pub security: super::security::ProfileSecurity,
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

    /// VFS address scheme for this profile. Neither FTPS variant is a
    /// separate scheme — both are the `ftp` provider with TLS enabled.
    pub fn scheme(&self) -> &str {
        match self.protocol.as_str() {
            "ftps" | "ftps-implicit" => "ftp",
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
        format!("{}://{}{}", self.scheme(), self.connection_key(), start)
    }
}

/// Live status of a connection (for the location selector and the
/// connection manager). `connected` is the registry's answer; `phase`,
/// `attempt` and `error` are the lifecycle around it (ticket 032), so a
/// reopened dialog renders a running attempt or a past failure instead
/// of resetting itself to "disconnected".
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ConnectionStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transport: Option<super::security::TransportInfo>,
    pub id: String,
    pub key: String,
    pub name: String,
    pub connected: bool,
    /// `sftp://user@host:port/start` — navigate here to browse.
    pub address: String,
    /// Where this profile stands right now (ticket 032).
    pub phase: Phase,
    /// Id of the running attempt, for `net_cancel_connect`.
    pub attempt: Option<String>,
    /// How the last attempt failed, typed (ticket 032).
    pub error: Option<ConnFailure>,
}

/// What `net_connect` produced. An attempt yields a live session, stops
/// at a host-key question (ticket 028), or reports that one was already
/// running for this profile (ticket 032) — the frontend branches on the
/// tag, never on message text.
#[derive(Serialize, Clone, TS)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ConnectOutcome {
    Connected(ConnectionStatus),
    HostKey(HostKeyChallenge),
    /// A duplicate request: this profile already has an attempt running.
    /// The caller shows that one rather than opening a second socket.
    Pending(ConnectionStatus),
}

// ---------- Persistence ----------

fn config_path() -> AppResult<PathBuf> {
    crate::config::path("connections.json")
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

fn store_profiles(profiles: &[Profile]) -> AppResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    let json = serde_json::to_vec_pretty(profiles).map_err(AppError::from)?;
    std::fs::write(&path, json).map_err(AppError::from)
}

// ---------- Registry ----------

/// Holds saved profiles and the live connections. Connections are keyed
/// by their provider's `store_id()` (`"<scheme>:<user@host:port>"`), so
/// one registry serves every protocol.
struct Registry {
    profiles: Vec<Profile>,
    live: HashMap<String, Arc<dyn Provider>>,
}

fn registry() -> &'static Mutex<Registry> {
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(Registry {
            profiles: load_profiles(),
            live: HashMap::new(),
        })
    })
}

/// Puts a provider into the live registry the way `net_connect` does.
/// The live provider tests need a registered session so that the
/// address-based lookups (`vfs::resolve`, `vfs::caps`) see the very
/// connection the test just opened (ticket 031).
#[cfg(test)]
pub(crate) fn register_live(provider: Arc<dyn Provider>) {
    registry()
        .lock()
        .unwrap()
        .live
        .insert(provider.store_id(), provider);
}

#[cfg(test)]
pub(crate) fn unregister_live(store: &str) {
    registry().lock().unwrap().live.remove(store);
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
pub fn remote_provider(scheme: &str, key: &str) -> AppResult<Arc<dyn Provider>> {
    let store_id = format!("{}:{}", scheme, key);
    let reg = registry().lock().unwrap();
    match reg.live.get(&store_id) {
        Some(p) => Ok(p.clone()),
        None => Err(AppError::from(format!(
            "Keine aktive {}-Verbindung für {} — bitte zuerst verbinden",
            scheme.to_uppercase(),
            key
        ))),
    }
}

/// Whether a profile's session is in the live registry right now.
fn is_live(profile: &Profile) -> bool {
    registry()
        .lock()
        .unwrap()
        .live
        .contains_key(&profile.store_id())
}

/// One profile's status, with its lifecycle reconciled against the
/// registry. The registry is the truth about a *live* session; the
/// lifecycle adds the states it cannot express.
fn status_of(profile: &Profile) -> ConnectionStatus {
    let live = is_live(profile);
    let transport = if live {
        registry()
            .lock()
            .ok()
            .and_then(|r| {
                r.live
                    .get(&profile.store_id())
                    .and_then(|p| p.transport_security())
            })
            .or_else(|| {
                (profile.protocol == "sftp")
                    .then(|| super::security::TransportInfo::connected(profile, true))
            })
    } else {
        None
    };
    // A session approved for another saved profile is not this profile's session,
    // even when both profiles happen to use the same VFS address.
    let own_session = transport
        .as_ref()
        .is_none_or(|info| info.profile_id == profile.id);
    let (phase, attempt, error) = lifecycle::reconcile(&profile.id, live && own_session);
    let transport = if own_session { transport } else { None };
    ConnectionStatus {
        transport,
        id: profile.id.clone(),
        key: profile.connection_key(),
        name: profile.name.clone(),
        connected: phase == Phase::Connected,
        address: profile.address(),
        phase,
        attempt,
        error,
    }
}

/// Current status of every saved profile.
///
/// Sessions whose transport is definitively gone are reaped first, so a
/// connection the peer dropped stops being reported as live the moment
/// anything asks — the manager and the location pickers therefore learn
/// about a loss at the same instant (ticket 032).
fn all_states() -> Vec<ConnectionStatus> {
    reap_dead_sessions();
    // The profile list is copied out first: `status_of` takes the
    // registry lock itself, and no lock may be held across that.
    let profiles = registry().lock().unwrap().profiles.clone();
    profiles.iter().map(status_of).collect()
}

/// Drops every live provider whose transport reports itself closed.
/// `transport_alive` is answered from local state, so this stays a cheap
/// check rather than a round trip per connection.
fn reap_dead_sessions() {
    let dead: Vec<String> = {
        let reg = registry().lock().unwrap();
        reg.live
            .iter()
            .filter(|(_, p)| !p.transport_alive())
            .map(|(id, _)| id.clone())
            .collect()
    };
    for store_id in dead {
        report_connection_lost(&store_id, "Verbindung zum Server verloren");
    }
}

/// Pushes the current lifecycle of every profile to the frontend.
///
/// One event carrying the whole list rather than per-profile deltas: the
/// list is short, and a single authoritative snapshot cannot leave the
/// manager and the location pickers disagreeing about who is connected.
pub fn emit_states(app: &AppHandle) {
    let _ = app.emit("net-state", all_states());
}

/// Records that a live connection turned out to be gone.
///
/// Called by a provider or a terminal session that hit a definitive
/// transport failure — not a missing file or a refused operation, which
/// say nothing about the connection. The session is removed once, its
/// shells are closed with it, and the profile ends in `failed` with the
/// reason, so nothing stays marked live after the link is lost.
pub fn report_connection_lost(store_id: &str, reason: &str) {
    let (removed, profile_id) = {
        let mut reg = registry().lock().unwrap();
        crate::commands::fs::cache::invalidate();
        let removed = reg.live.remove(store_id).is_some();
        let profile_id = reg
            .profiles
            .iter()
            .find(|p| p.store_id() == store_id)
            .map(|p| p.id.clone());
        (removed, profile_id)
    };
    if !removed {
        // Already reported — "mark it once", not once per failing call.
        return;
    }
    // Outside the lock: closing a shell is network I/O.
    super::terminal::close_for_store(store_id);
    if let Some(id) = profile_id {
        lifecycle::set_disconnected(&id, Some(ConnFailure::new(FailureKind::Protocol, reason)));
    }
}

// ---------- Commands ----------

/// Lists saved connection profiles.
#[tauri::command]
pub fn net_list_profiles() -> Vec<Profile> {
    registry().lock().unwrap().profiles.clone()
}

/// Creates or updates a profile. When `secret` is `Some`, it goes to the
/// configured credential store under the profile id (never into the JSON).
#[tauri::command]
pub fn net_save_profile(profile: Profile, secret: Option<String>) -> AppResult<()> {
    crate::commands::fs::cache::invalidate();
    super::certificate::approved_der(&profile)?;
    lifecycle::cancel_for_profile(&profile.id);
    // Written before the registry lock: a locked KeePass file fails here,
    // and the profile list must not be touched when the secret is lost.
    if let Some(s) = &secret {
        credentials::store().set(&profile.id, s)?;
    }
    let mut reg = registry().lock().unwrap();
    if let Some(existing) = reg.profiles.iter().find(|p| p.id == profile.id) {
        let key = existing.store_id();
        crate::commands::fs::cache::invalidate();
        reg.live.remove(&key);
    }
    match reg.profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => reg.profiles.push(profile),
    }
    store_profiles(&reg.profiles)
}

/// Deletes a profile and its stored secret (disconnecting if live).
#[tauri::command]
pub fn net_delete_profile(id: String) -> AppResult<()> {
    crate::commands::fs::cache::invalidate();
    // A profile that is still connecting loses its attempt with it.
    lifecycle::cancel_for_profile(&id);
    let mut reg = registry().lock().unwrap();
    let mut closed = None;
    if let Some(pos) = reg.profiles.iter().position(|p| p.id == id) {
        let store_id = reg.profiles[pos].store_id();
        crate::commands::fs::cache::invalidate();
        reg.live.remove(&store_id);
        reg.profiles.remove(pos);
        closed = Some(store_id);
    }
    let _ = credentials::store().delete(&id);
    let result = store_profiles(&reg.profiles);
    drop(reg);
    if let Some(store_id) = closed {
        super::terminal::close_for_store(&store_id);
    }
    lifecycle::forget(&id);
    result
}

/// Looks up a saved profile without holding the registry lock any
/// longer than the copy takes.
fn profile_by_id(id: &str) -> AppResult<Profile> {
    registry()
        .lock()
        .unwrap()
        .profiles
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| AppError::from(format!("Profil {} nicht gefunden", id)))
}

/// What a finished attempt produced, before the lifecycle decides
/// whether it may become the live session.
enum Opened {
    Provider(Arc<dyn Provider>),
    HostKey(HostKeyChallenge),
}

/// The whole connect sequence for one attempt, off the UI thread.
///
/// Each phase is preceded by a cancellation check, so a cancel taken
/// during the previous phase stops the next one from starting. Nothing
/// here touches the registry: the result travels back to [`net_connect`]
/// and only `lifecycle::settle` may install it.
fn run_attempt(attempt: &Attempt, profile: &Profile) -> Result<Opened, ConnFailure> {
    attempt.check()?;
    super::security::check(profile).map_err(|e| ConnFailure::new(FailureKind::Protocol, e))?;

    // 1. The secret. A keychain prompt or a locked KeePass file is a
    //    failure of its own kind, not a protocol error — and it happens
    //    before any socket, with no registry lock held anywhere.
    let secret = credentials::store()
        .get(&profile.id)
        .map_err(|e| ConnFailure::new(FailureKind::Keychain, e))?;
    attempt.check()?;

    // 2. The transport. Reaching the port is timed uniformly (15 s),
    //    because most protocol clients cannot express a connect timeout.
    lifecycle::probe_transport(&profile.host, profile.port)?;
    attempt.check()?;

    // 3. Handshake, authentication, host verification and the provider's
    //    own readiness probe — all inside the client's `connect`.
    let opened: Result<Arc<dyn Provider>, ConnectError> = match profile.protocol.as_str() {
        "sftp" => crate::vfs::sftp::connect(profile, secret.as_deref()),
        "ftp" | "ftps" | "ftps-implicit" => {
            crate::vfs::ftp::connect(profile, secret.as_deref()).map_err(ConnectError::from)
        }
        "webdav" => {
            crate::vfs::webdav::connect(profile, secret.as_deref()).map_err(ConnectError::from)
        }
        "smb" => crate::vfs::smb::connect(profile, secret.as_deref()).map_err(ConnectError::from),
        other => {
            return Err(ConnFailure::new(
                FailureKind::Protocol,
                format!("Protokoll '{}' wird noch nicht unterstützt", other),
            ))
        }
    };

    match opened {
        Ok(provider) => Ok(Opened::Provider(provider)),
        Err(ConnectError::HostKey(challenge)) => Ok(Opened::HostKey(*challenge)),
        Err(ConnectError::Message(message)) => Err(lifecycle::classify_protocol_error(&message)),
    }
}

/// Opens a connection for a profile and returns its status.
///
/// The attempt runs on a worker thread under a 45-second cap, so the
/// command never outlives the budget however the server behaves. It can
/// be stopped with `net_cancel_connect`, a second click finds the
/// running attempt instead of opening another socket, and the provider
/// enters the registry only in `lifecycle::settle` — so a superseded or
/// cancelled result cannot replace a working session, and a failed
/// reconnect leaves the previous one connected (ticket 032).
///
/// An SSH host whose key is unknown or has changed stops with a host-key
/// challenge instead; nothing has been authenticated at that point
/// (ticket 028).
#[tauri::command]
pub fn net_connect(app: AppHandle, id: String) -> AppResult<ConnectOutcome> {
    let profile = profile_by_id(&id)?;
    super::security::check(&profile)?;

    let attempt = match lifecycle::begin(&id) {
        Start::Started(a) => a,
        // A duplicate click: report the attempt that is already running,
        // so the caller shows (and can cancel) that one instead of
        // opening a second socket.
        Start::AlreadyRunning(running) => {
            let mut status = status_of(&profile);
            status.phase = Phase::Connecting;
            status.attempt = Some(running.id);
            status.connected = false;
            return Ok(ConnectOutcome::Pending(status));
        }
    };
    // The pending state is visible before the first packet leaves.
    emit_states(&app);

    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Opened, ConnFailure>>(1);
    let worker = {
        let attempt = attempt.clone();
        let profile = profile.clone();
        // A plain thread, not the shared Tokio runtime: the protocol
        // clients bridge async work through `block_on` internally, which
        // must not run inside a runtime worker.
        std::thread::spawn(move || {
            let outcome = run_attempt(&attempt, &profile);
            // The receiver is gone once the caller hit the overall
            // timeout. Settle regardless — that is what closes a late
            // provider and returns the profile to a stable state.
            if tx.send(outcome).is_err() {
                lifecycle::settle(
                    &attempt,
                    Err(ConnFailure::new(
                        FailureKind::Timeout,
                        "Verbindungsaufbau hat zu lange gedauert",
                    )),
                    || {},
                );
            }
        })
    };

    let result = rx.recv_timeout(OVERALL_TIMEOUT);
    let outcome = match result {
        Ok(outcome) => outcome,
        // Timed out: the worker keeps running and settles on its own,
        // dropping whatever it opened. The caller is free immediately.
        Err(_) => {
            let failure = ConnFailure::new(
                FailureKind::Timeout,
                format!(
                    "Keine Antwort von {} innerhalb von {} s",
                    profile.host,
                    OVERALL_TIMEOUT.as_secs()
                ),
            );
            lifecycle::cancel(&attempt.id);
            lifecycle::settle(&attempt, Err(failure.clone()), || {});
            emit_states(&app);
            return Err(failure.error);
        }
    };
    // The worker is done the moment it sent; joining cannot block.
    let _ = worker.join();

    match outcome {
        Ok(Opened::Provider(provider)) => {
            let store_id = provider.store_id();
            // `settle` is the only place a provider becomes live, and it
            // holds the lifecycle lock for exactly that decision.
            let installed = lifecycle::settle(&attempt, Ok(()), || {
                registry()
                    .lock()
                    .unwrap()
                    .live
                    .insert(store_id.clone(), provider.clone());
            });
            emit_states(&app);
            if !installed {
                // Cancelled or superseded — dropping `provider` here
                // closes the connection this attempt opened.
                drop(provider);
                return Err(ConnFailure::new(
                    FailureKind::Cancelled,
                    "Verbindungsaufbau abgebrochen",
                )
                .error);
            }
            Ok(ConnectOutcome::Connected(status_of(&profile)))
        }
        Ok(Opened::HostKey(challenge)) => {
            // Not a failure: the identity question is a state of its own
            // and the profile waits for the user's answer.
            lifecycle::settle(
                &attempt,
                Err(ConnFailure::new(
                    FailureKind::HostKey,
                    ConnectError::HostKey(Box::new(challenge.clone())).to_app_error(),
                )),
                || {},
            );
            emit_states(&app);
            Ok(ConnectOutcome::HostKey(challenge))
        }
        Err(failure) => {
            lifecycle::settle(&attempt, Err(failure.clone()), || {});
            emit_states(&app);
            Err(failure.error)
        }
    }
}

/// Stops a running connection attempt. Idempotent: an unknown, finished
/// or already-cancelled id is a no-op and never an error, so a second
/// click on "cancel" cannot fail (ticket 032).
#[tauri::command]
pub fn net_cancel_connect(app: AppHandle, attempt: String) {
    lifecycle::cancel(&attempt);
    emit_states(&app);
}

/// Persists the host key the user just confirmed and retries the
/// connection. `token` is the one-shot value from the challenge, so only
/// the key hortcmd actually displayed can be trusted; a failed write
/// aborts before any authentication is attempted (ticket 028).
#[tauri::command]
pub fn net_trust_host_key(app: AppHandle, id: String, token: String) -> AppResult<ConnectOutcome> {
    hostkey::trust(&token)?;
    net_connect(app, id)
}

/// Drops the host keys hortcmd stores for a profile's endpoint. The
/// explicit action behind a changed host key — the mismatch dialog
/// itself offers no bypass. The user's own `known_hosts` files are only
/// reported on, never rewritten.
#[tauri::command]
pub fn net_forget_host_key(id: String) -> AppResult<ForgetResult> {
    crate::commands::fs::cache::invalidate();
    let profile = profile_by_id(&id)?;
    hostkey::forget(&HostId::new(&profile.host, profile.port))
}

/// Disconnects a live connection (by profile id). Profile stays saved.
/// Open shell sessions on that connection are closed with it (026), so
/// no channel outlives the connection it rides on.
#[tauri::command]
pub fn net_disconnect(app: AppHandle, id: String) -> AppResult<()> {
    crate::commands::fs::cache::invalidate();
    // A second disconnect while the first is still tearing down would
    // race it; the request is simply dropped (ticket 032).
    if lifecycle::snapshot(&id).0 == Phase::Disconnecting {
        return Ok(());
    }
    // A pending attempt is stopped first: disconnecting a profile that
    // is still connecting must not leave that attempt free to install
    // its provider afterwards (ticket 032).
    lifecycle::cancel_for_profile(&id);
    lifecycle::begin_disconnect(&id);
    emit_states(&app);

    let store_id = {
        let mut reg = registry().lock().unwrap();
        let store_id = reg
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|profile| profile.store_id());
        if let Some(store_id) = &store_id {
            crate::commands::fs::cache::invalidate();
            reg.live.remove(store_id);
        }
        store_id
    };
    // Outside the lock: closing a shell channel is network I/O, and no
    // mutex is held while waiting for it.
    if let Some(store_id) = store_id {
        super::terminal::close_for_store(&store_id);
    }
    lifecycle::set_disconnected(&id, None);
    emit_states(&app);
    Ok(())
}

/// Lists saved profiles with their live-connection status and lifecycle
/// (ticket 032). This is the authoritative state the dialog adopts when
/// it reopens — a running attempt or a recorded failure survives the
/// refresh instead of being reset to "disconnected".
#[tauri::command]
pub fn net_list_connections() -> Vec<ConnectionStatus> {
    connection_states()
}

pub(crate) fn connection_states() -> Vec<ConnectionStatus> {
    all_states()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile {
            security: Default::default(),
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
    fn explicit_webdav_schemes_share_the_registry_address() {
        for host in [
            "https://dav.example",
            "http://dav.example",
            "https://dav.example/",
        ] {
            let mut p = profile();
            p.protocol = "webdav".into();
            p.host = host.into();
            p.port = 8443;
            p.start_path = "/shared".into();
            assert_eq!(p.address(), "webdav://alice@dav.example:8443/shared");
            let address = crate::vfs::parse_address(&p.address()).unwrap();
            assert_eq!(
                address.connection_key().as_deref(),
                Some(p.connection_key().as_str())
            );
            assert_eq!(p.host, host);
        }
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
