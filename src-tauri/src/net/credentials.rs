// ============================================================
// Credential storage (tickets 021 + 027).
//
// Secrets (passwords / key passphrases) are kept out of plain text.
// The `CredentialStore` trait keeps the callers backend-agnostic, so
// the backend can be swapped without a single change at the call sites.
//
// Two backends exist:
//
//   * `KeychainStore` — the **default** (the `keyring` crate): macOS
//     Keychain, Windows Credential Manager, Linux Secret Service. A
//     missing Secret Service (headless Linux) is reported as a clear
//     error — there is deliberately **no** silent plain-text fallback.
//   * `KeepassStore` — optional (ticket 027): a portable, encrypted
//     `.kdbx` file the user owns and can sync, interoperable with
//     KeePassXC. No security gain over the keychain, but portability.
//
// `Store` routes every call to the backend the user picked; the choice
// lives in `credentials.json` next to `connections.json`. Without an
// explicit choice the keychain stays active.
//
// The KeePass master password is held **in memory only** while the file
// is unlocked. Persisting it is a separate, clearly labeled opt-in
// (`master_in_keychain`) — otherwise the file would be no safer than
// the keychain it replaces.
// ============================================================

use crate::errors::{code, AppError, AppResult, ErrorAction};
use keepass::db::{fields, EntryId, GroupId, GroupMut, Times};
use keepass::{Database, DatabaseKey};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

/// Keychain service name under which all connection secrets are filed.
const SERVICE: &str = "com.hortcmd.app";

/// Group inside the `.kdbx` that holds hortcmd's connection secrets.
/// Entries elsewhere in the file are still found (KeePassXC interop),
/// new ones land here so a shared database stays tidy.
const KP_GROUP: &str = "hortcmd";

/// Marker every "KeePass file still locked" error carries. The frontend
/// keys its unlock prompt off it, so the token must stay stable.
#[allow(unused_imports)] // the name other modules and tests use
pub use crate::errors::code::CRED_LOCKED as LOCKED;

/// "The KeePass file is locked" — not a failure but a question. The
/// frontend parks the action behind the unlock prompt and replays it;
/// since ticket 036 it recognizes this by code rather than by a marker
/// inside a sentence.
pub fn locked() -> AppError {
    AppError::new(code::CRED_LOCKED).action(ErrorAction::ProvidePassword)
}

/// Keychain id for the KeePass master password — only ever written when
/// the user explicitly opts in (`CredConfig::master_in_keychain`).
const MASTER_ID: &str = "__keepass_master__";

/// Backend-agnostic secret storage keyed by connection id.
pub trait CredentialStore: Send + Sync {
    /// Returns the stored secret, or `None` if none is stored.
    /// A backend/access failure is an `Err`, never a silent `None`.
    fn get(&self, id: &str) -> AppResult<Option<String>>;

    /// Stores (or replaces) the secret for a connection id.
    fn set(&self, id: &str, secret: &str) -> AppResult<()>;

    /// Removes the secret; removing a missing entry is not an error.
    fn delete(&self, id: &str) -> AppResult<()>;
}

// ---------- Backend selection ----------

/// Which backend holds the secrets, plus everything the KeePass backend
/// needs to find its file. Never contains the master password.
#[derive(Serialize, Deserialize, Clone, TS)]
#[serde(default)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CredConfig {
    /// `"keychain"` (default) or `"keepass"`.
    pub backend: String,
    /// Path to the `.kdbx` file (KeePass backend only).
    pub kdbx_path: String,
    /// Optional key file — alongside or instead of the master password.
    pub key_file: String,
    /// Opt-in: keep the master password in the OS keychain so the file
    /// unlocks without a prompt. Clearly labeled in the UI.
    pub master_in_keychain: bool,
}

impl Default for CredConfig {
    fn default() -> Self {
        Self {
            backend: "keychain".to_string(),
            kdbx_path: String::new(),
            key_file: String::new(),
            master_in_keychain: false,
        }
    }
}

impl CredConfig {
    fn is_keepass(&self) -> bool {
        self.backend == "keepass"
    }

    fn key_file_path(&self) -> Option<PathBuf> {
        let trimmed = self.key_file.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    }
}

/// Backend choice plus the live lock state, for the settings UI.
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CredStatus {
    pub config: CredConfig,
    /// The `.kdbx` is open in memory right now.
    pub unlocked: bool,
    /// KeePass is active but locked — a master password is needed.
    pub needs_unlock: bool,
}

fn config_path() -> AppResult<PathBuf> {
    crate::config::path("credentials.json")
}

fn load_config() -> CredConfig {
    let Ok(path) = config_path() else {
        return CredConfig::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => CredConfig::default(),
    }
}

fn store_config(cfg: &CredConfig) -> AppResult<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::from)?;
    }
    let json = serde_json::to_vec_pretty(cfg).map_err(AppError::from)?;
    std::fs::write(&path, json).map_err(AppError::from)
}

// ---------- Router ----------

/// The store the whole app talks to. Dispatches to the configured
/// backend; the callers never learn which one is active.
pub struct Store {
    config: Mutex<CredConfig>,
    keychain: KeychainStore,
    keepass: KeepassStore,
}

/// Process-wide credential store.
pub fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| Store {
        config: Mutex::new(load_config()),
        keychain: KeychainStore::new(),
        keepass: KeepassStore::default(),
    })
}

impl Store {
    fn config(&self) -> CredConfig {
        self.config.lock().unwrap().clone()
    }

    pub fn status(&self) -> CredStatus {
        let config = self.config();
        let unlocked = self.keepass.is_unlocked();
        CredStatus {
            needs_unlock: config.is_keepass() && !unlocked,
            unlocked,
            config,
        }
    }

    /// Switches the backend. Locks any open `.kdbx` and drops a stored
    /// master password whenever it is no longer wanted.
    pub fn set_config(&self, mut cfg: CredConfig) -> AppResult<CredStatus> {
        if cfg.backend != "keychain" && cfg.backend != "keepass" {
            return Err(AppError::from(format!(
                "Unbekannter Speicher '{}'",
                cfg.backend
            )));
        }
        cfg.kdbx_path = cfg.kdbx_path.trim().to_string();
        cfg.key_file = cfg.key_file.trim().to_string();
        if cfg.is_keepass() && cfg.kdbx_path.is_empty() {
            return Err(AppError::from("Bitte eine .kdbx-Datei angeben"));
        }
        self.keepass.lock();
        if !cfg.is_keepass() || !cfg.master_in_keychain {
            let _ = self.keychain.delete(MASTER_ID);
        }
        store_config(&cfg)?;
        *self.config.lock().unwrap() = cfg;
        Ok(self.status())
    }

    /// Opens the `.kdbx`. `remember` keeps it open for the session,
    /// otherwise it re-locks after the next credential access.
    /// `create` allows starting a fresh database at the configured path.
    pub fn unlock(&self, password: &str, remember: bool, create: bool) -> AppResult<CredStatus> {
        self.unlock_with_keychain(password, remember, create, |password| {
            self.keychain.set(MASTER_ID, password)
        })
    }

    fn unlock_with_keychain(
        &self,
        password: &str,
        remember: bool,
        create: bool,
        save_master: impl FnOnce(&str) -> AppResult<()>,
    ) -> AppResult<CredStatus> {
        let cfg = self.config();
        if !cfg.is_keepass() {
            return Err(AppError::from("Der KeePass-Speicher ist nicht aktiv"));
        }
        // A remembered master password in the keychain outlives the
        // session, so it only ever makes sense together with `remember`.
        let remember = remember || cfg.master_in_keychain;
        self.keepass.unlock(&cfg, password, remember, create)?;
        if cfg.master_in_keychain {
            if let Err(error) = save_master(password) {
                self.keepass.lock();
                return Err(error);
            }
        }
        Ok(self.status())
    }

    /// Forgets the master password and closes the file.
    pub fn lock(&self) -> CredStatus {
        self.keepass.lock();
        self.status()
    }

    /// Unlocks silently from the keychain when the user opted into that.
    /// Any other locked state stays an error the UI turns into a prompt.
    fn ensure_unlocked(&self, cfg: &CredConfig) -> AppResult<()> {
        if self.keepass.is_unlocked() || !cfg.master_in_keychain {
            return Ok(());
        }
        match self.keychain.get(MASTER_ID)? {
            Some(master) => self.keepass.unlock(cfg, &master, true, false),
            None => Ok(()),
        }
    }

    /// The backend in charge right now, unlocked if it can be.
    fn backend(&self) -> AppResult<&dyn CredentialStore> {
        let cfg = self.config();
        if !cfg.is_keepass() {
            return Ok(&self.keychain);
        }
        self.ensure_unlocked(&cfg)?;
        Ok(&self.keepass)
    }
}

impl CredentialStore for Store {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        self.backend()?.get(id)
    }
    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        self.backend()?.set(id, secret)
    }
    fn delete(&self, id: &str) -> AppResult<()> {
        self.backend()?.delete(id)
    }
}

// ---------- OS keychain backend (default) ----------

/// OS-keychain backed store (default).
pub struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_string(),
        }
    }

    fn entry(&self, id: &str) -> AppResult<Entry> {
        Entry::new(&self.service, id).map_err(explain)
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeychainStore {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        match self.entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(explain(e)),
        }
    }

    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        self.entry(id)?.set_password(secret).map_err(explain)
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        match self.entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(explain(e)),
        }
    }
}

/// Turns a keyring error into a user-facing message. On Linux a
/// missing/locked Secret Service is the common failure, so the remedy
/// is spelled out instead of leaking a raw platform error.
/// The OS-keychain boundary (ticket 036). "No keychain service at all"
/// is worth its own code: on Linux it is a setup problem the user can
/// fix, and it must not read like a wrong password.
fn explain(e: keyring::Error) -> AppError {
    match e {
        keyring::Error::NoEntry => AppError::new(code::CRED_NOT_FOUND),
        keyring::Error::NoStorageAccess(inner) => AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(no_service_hint(&inner.to_string())),
        keyring::Error::PlatformFailure(inner) => AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(no_service_hint(&inner.to_string())),
        other => AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(other),
    }
}

#[cfg(target_os = "linux")]
fn no_service_hint(detail: &str) -> String {
    format!(
        "Kein Secret Service verfügbar ({}). Bitte einen Schlüsselbund-Dienst \
         (z. B. gnome-keyring / KWallet) installieren und entsperren — \
         Passwörter werden bewusst nicht im Klartext gespeichert.",
        detail
    )
}

#[cfg(not(target_os = "linux"))]
fn no_service_hint(detail: &str) -> String {
    format!("Schlüsselbund nicht zugänglich: {}", detail)
}

// ---------- KeePass backend (ticket 027) ----------

/// The unlocked `.kdbx`, held in memory while the session lasts.
/// Argon2 makes opening deliberately expensive, so the parsed database
/// is cached instead of being re-derived per credential access.
struct Session {
    path: PathBuf,
    key_file: Option<PathBuf>,
    password: String,
    db: Database,
    /// "Do not remember": re-lock after the next credential access.
    one_shot: bool,
}

impl Session {
    fn key(&self) -> AppResult<DatabaseKey> {
        database_key(&self.password, self.key_file.as_deref())
    }
}

/// Portable `.kdbx` backend. Locked until `unlock` succeeds; every
/// access while locked is an error carrying the [`LOCKED`] marker so the
/// UI can ask for the master password and retry.
#[derive(Default)]
pub struct KeepassStore {
    open: Mutex<Option<Session>>,
}

impl KeepassStore {
    pub fn is_unlocked(&self) -> bool {
        self.open.lock().unwrap().is_some()
    }

    pub fn lock(&self) {
        *self.open.lock().unwrap() = None;
    }

    /// Opens (or, with `create`, initialises) the configured `.kdbx`.
    fn unlock(
        &self,
        cfg: &CredConfig,
        password: &str,
        remember: bool,
        create: bool,
    ) -> AppResult<()> {
        let path = PathBuf::from(cfg.kdbx_path.trim());
        if path.as_os_str().is_empty() {
            return Err(AppError::from("Bitte eine .kdbx-Datei angeben"));
        }
        let key_file = cfg.key_file_path();
        if let Some(kf) = &key_file {
            if !kf.exists() {
                return Err(AppError::from(format!(
                    "Schlüsseldatei nicht gefunden: {}",
                    kf.display()
                )));
            }
        }
        let key = database_key(password, key_file.as_deref())?;

        let db = if path.exists() {
            let mut file = std::fs::File::open(&path).map_err(|e| {
                AppError::from(format!("{} lässt sich nicht öffnen: {}", path.display(), e))
            })?;
            Database::open(&mut file, key).map_err(open_error)?
        } else if create {
            let db = new_database();
            write_atomic(&path, &db, key)?;
            db
        } else {
            return Err(AppError::new(code::CRED_NOT_FOUND)
                .with("path", path.display())
                .action(ErrorAction::Report));
        };

        *self.open.lock().unwrap() = Some(Session {
            path,
            key_file,
            password: password.to_string(),
            db,
            one_shot: !remember,
        });
        Ok(())
    }

    /// Runs `f` against the open database. A one-shot session is dropped
    /// afterwards, so the next access asks for the master password again.
    fn with_session<T>(&self, f: impl FnOnce(&mut Session) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self.open.lock().unwrap();
        let session = guard.as_mut().ok_or_else(locked)?;
        let result = f(session);
        if guard.as_ref().is_some_and(|s| s.one_shot) {
            *guard = None;
        }
        result
    }
}

impl CredentialStore for KeepassStore {
    fn get(&self, id: &str) -> AppResult<Option<String>> {
        self.with_session(|s| {
            Ok(find_entry(&s.db, id)
                .and_then(|id| s.db.entry(id))
                .and_then(|e| e.get_password().map(str::to_string)))
        })
    }

    fn set(&self, id: &str, secret: &str) -> AppResult<()> {
        self.with_session(|s| {
            match find_entry(&s.db, id) {
                // An existing entry is updated where the user filed it.
                Some(found) => {
                    let mut entry = s.db.entry_mut(found).ok_or_else(missing_entry)?;
                    entry.set_protected(fields::PASSWORD, secret);
                    entry.times.last_modification = Some(Times::now());
                }
                None => {
                    let group = own_group(&mut s.db);
                    let mut group = s.db.group_mut(group).ok_or_else(missing_entry)?;
                    let mut entry = group.add_entry();
                    entry.set_icon_builtin(0);
                    entry.set_unprotected(fields::TITLE, id);
                    entry.set_protected(fields::PASSWORD, secret);
                    entry.set_unprotected(fields::NOTES, "hortcmd: Verbindungs-Zugangsdaten");
                }
            }
            write_atomic(&s.path, &s.db, s.key()?)
        })
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        self.with_session(|s| {
            // Removing a missing entry is not an error — and it must not
            // rewrite the file either.
            let Some(found) = find_entry(&s.db, id) else {
                return Ok(());
            };
            s.db.entry_mut(found).ok_or_else(missing_entry)?.remove();
            write_atomic(&s.path, &s.db, s.key()?)
        })
    }
}

/// A fresh KDBX 4 database (Argon2, AES-256, ChaCha20 inner stream —
/// the crate's KeePass-compatible defaults).
///
/// `Meta` is filled in explicitly: the crate writes `None` fields as
/// empty XML elements, and KeePassXC refuses to open a file whose
/// numeric/boolean metadata is blank. Everything here mirrors what
/// KeePassXC itself writes for a new database.
fn new_database() -> Database {
    use keepass::db::MemoryProtection;

    let now = Times::now();
    let mut db = Database::new();
    db.meta.generator = Some("hortcmd".to_string());
    db.meta.database_name = Some("hortcmd".to_string());
    db.meta.database_name_changed = Some(now);
    db.meta.database_description = Some(String::new());
    db.meta.database_description_changed = Some(now);
    db.meta.default_username = Some(String::new());
    db.meta.default_username_changed = Some(now);
    db.meta.maintenance_history_days = Some(365);
    db.meta.master_key_changed = Some(now);
    db.meta.master_key_change_rec = Some(-1);
    db.meta.master_key_change_force = Some(-1);
    db.meta.memory_protection = Some(MemoryProtection::default());
    db.meta.recyclebin_enabled = Some(true);
    db.meta.recyclebin_uuid = Some(Default::default());
    db.meta.recyclebin_changed = Some(now);
    db.meta.entry_templates_group = Some(Default::default());
    db.meta.entry_templates_group_changed = Some(now);
    db.meta.last_selected_group = Some(Default::default());
    db.meta.last_top_visible_group = Some(Default::default());
    db.meta.history_max_items = Some(10);
    db.meta.history_max_size = Some(6 * 1024 * 1024);
    db.meta.settings_changed = Some(now);
    decorate_group(&mut db.root_mut());
    db
}

/// Fills the fields KeePassXC expects to be present on a group.
fn decorate_group(group: &mut GroupMut<'_>) {
    group.notes = Some(String::new());
    group.set_icon_builtin(48);
    group.enable_autotype = Some(true);
    group.enable_searching = Some(true);
    group.is_expanded = true;
}

/// Builds the KeePass key from master password and/or key file.
fn database_key(password: &str, key_file: Option<&Path>) -> AppResult<DatabaseKey> {
    let mut key = DatabaseKey::new();
    if !password.is_empty() {
        key = key.with_password(password);
    }
    if let Some(path) = key_file {
        let mut file = std::fs::File::open(path).map_err(|e| {
            AppError::from(format!(
                "Schlüsseldatei {} nicht lesbar: {}",
                path.display(),
                e
            ))
        })?;
        key = key.with_keyfile(&mut file).map_err(|e| {
            AppError::from(format!(
                "Schlüsseldatei {} nicht lesbar: {}",
                path.display(),
                e
            ))
        })?;
    }
    if key.is_empty() {
        return Err(AppError::new(code::CRED_WRONG_PASSWORD).action(ErrorAction::ProvidePassword));
    }
    Ok(key)
}

/// The KeePass boundary. A wrong master password is by far the common
/// case and gets its own code — it must not read like a corrupt file,
/// and the frontend re-prompts on it rather than giving up.
fn open_error(e: keepass::error::DatabaseOpenError) -> AppError {
    match e {
        keepass::error::DatabaseOpenError::Key(_) => {
            AppError::new(code::CRED_WRONG_PASSWORD).action(ErrorAction::ProvidePassword)
        }
        other => AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(other),
    }
}

/// Writes the database through a temporary sibling and renames it into
/// place, so an abort (crash, full disk) leaves the original intact.
///
/// KDBX 3 files are readable but not writable — that format predates
/// Argon2 and only KDBX 4 is written, so the message names the remedy
/// instead of leaking a raw "unsupported version".
fn write_atomic(path: &Path, db: &Database, key: DatabaseKey) -> AppResult<()> {
    let mut buffer = Vec::new();
    db.save(&mut buffer, key).map_err(|e| match e {
        // Its own code: the remedy is specific (upgrade the database in
        // KeePassXC) and must not read like a generic write failure.
        keepass::error::DatabaseSaveError::UnsupportedVersion => {
            AppError::new(code::CRED_READ_ONLY_FORMAT).action(ErrorAction::Report)
        }
        other => AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(other),
    })?;

    let tmp = path.with_extension("kdbx.hortcmd-tmp");
    let write = |tmp: &Path| -> std::io::Result<()> {
        let mut file = std::fs::File::create(tmp)?;
        file.write_all(&buffer)?;
        file.sync_all()
    };
    if let Err(e) = write(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::from(format!(
            "KeePass-Datei nicht schreibbar: {}",
            e
        )));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::new(code::CRED_STORE)
            .action(ErrorAction::Report)
            .detail(e)
    })
}

/// hortcmd's own group, created on first write. Entries the user filed
/// elsewhere are still found by the lookup below (KeePassXC interop).
fn own_group(db: &mut Database) -> GroupId {
    if let Some(group) = db.root().group_by_name(KP_GROUP) {
        return group.id();
    }
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = KP_GROUP.to_string();
    decorate_group(&mut group);
    group.id()
}

/// Lookup by entry title across the whole database. Entries live in one
/// flat map, so a single pass covers every group — including the ones the
/// user filed credentials in themselves.
fn find_entry(db: &Database, title: &str) -> Option<EntryId> {
    db.iter_all_entries()
        .find(|entry| entry.get_title() == Some(title))
        .map(|entry| entry.id())
}

/// An id that came out of `find_entry` or `own_group` and no longer
/// resolves means the database was mutated behind our back.
fn missing_entry() -> AppError {
    AppError::new(code::CRED_STORE).action(ErrorAction::Report)
}

// ---------- Commands ----------

/// Current backend choice plus lock state.
#[tauri::command]
pub fn net_cred_status() -> CredStatus {
    store().status()
}

/// Switches the credential backend (and its KeePass file).
#[tauri::command]
pub fn net_cred_set_backend(config: CredConfig) -> AppResult<CredStatus> {
    store().set_config(config)
}

/// Unlocks the configured `.kdbx` with the master password.
#[tauri::command]
pub fn net_cred_unlock(password: String, remember: bool, create: bool) -> AppResult<CredStatus> {
    store().unlock(&password, remember, create)
}

/// Closes the `.kdbx` and forgets the master password.
#[tauri::command]
pub fn net_cred_lock() -> CredStatus {
    store().lock()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// In-memory store for deterministic tests (no OS keychain).
    #[derive(Default)]
    pub(crate) struct MemoryStore {
        map: Mutex<HashMap<String, String>>,
    }

    impl CredentialStore for MemoryStore {
        fn get(&self, id: &str) -> AppResult<Option<String>> {
            Ok(self.map.lock().unwrap().get(id).cloned())
        }
        fn set(&self, id: &str, secret: &str) -> AppResult<()> {
            self.map
                .lock()
                .unwrap()
                .insert(id.to_string(), secret.to_string());
            Ok(())
        }
        fn delete(&self, id: &str) -> AppResult<()> {
            self.map.lock().unwrap().remove(id);
            Ok(())
        }
    }

    #[test]
    fn store_contract_roundtrip() {
        let s = MemoryStore::default();
        assert_eq!(s.get("c1").unwrap(), None);
        s.set("c1", "hunter2").unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("hunter2"));
        s.set("c1", "changed").unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("changed"));
        s.delete("c1").unwrap();
        assert_eq!(s.get("c1").unwrap(), None);
        // Deleting a missing entry is not an error.
        s.delete("c1").unwrap();
    }

    /// Live OS-keychain round-trip. Ignored by default because a test
    /// binary may trigger an interactive keychain prompt / access
    /// failure; run manually with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn keychain_roundtrip_live() {
        let s = KeychainStore::new();
        let id = "hortcmd-test-connection";
        s.set(id, "sekret").unwrap();
        assert_eq!(s.get(id).unwrap().as_deref(), Some("sekret"));
        s.delete(id).unwrap();
        assert_eq!(s.get(id).unwrap(), None);
    }

    // ----- KeePass backend (ticket 027) -----

    /// A `.kdbx` path in a fresh temp dir, plus the config pointing at it.
    fn kdbx_config(name: &str) -> (PathBuf, CredConfig) {
        let dir =
            std::env::temp_dir().join(format!("hortcmd-kdbx-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.kdbx");
        let _ = std::fs::remove_file(&path);
        let cfg = CredConfig {
            backend: "keepass".to_string(),
            kdbx_path: path.to_string_lossy().to_string(),
            ..CredConfig::default()
        };
        (path, cfg)
    }

    #[test]
    fn failed_master_keychain_write_relocks_the_vault() {
        let (path, mut cfg) = kdbx_config("keychain-failure");
        cfg.master_in_keychain = true;
        let store = Store {
            config: Mutex::new(cfg),
            keychain: KeychainStore::new(),
            keepass: KeepassStore::default(),
        };
        let result = store.unlock_with_keychain("master", false, true, |_| {
            assert!(store.keepass.is_unlocked());
            Err(AppError::new(code::CRED_STORE))
        });
        assert!(result.err().unwrap().is(code::CRED_STORE));
        assert!(!store.status().unlocked);
        assert!(store.status().needs_unlock);
        assert!(store.keepass.get("secret").unwrap_err().is(LOCKED));
        let status = store
            .unlock_with_keychain("master", false, false, |password| {
                assert_eq!(password, "master");
                Ok(())
            })
            .unwrap();
        assert!(status.unlocked);
        assert!(!status.needs_unlock);
        store.lock();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn keepass_roundtrip_and_persistence() {
        let (path, cfg) = kdbx_config("roundtrip");
        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, true).unwrap();

        assert_eq!(s.get("c1").unwrap(), None);
        s.set("c1", "hunter2").unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("hunter2"));
        s.set("c1", "changed").unwrap();
        s.set("c2", "second").unwrap();
        s.delete("c2").unwrap();
        // Deleting a missing entry stays a no-op.
        s.delete("c2").unwrap();

        // Re-open from disk: the writes survived the process boundary.
        let fresh = KeepassStore::default();
        fresh.unlock(&cfg, "master", true, false).unwrap();
        assert_eq!(fresh.get("c1").unwrap().as_deref(), Some("changed"));
        assert_eq!(fresh.get("c2").unwrap(), None);

        // The secret never lands in plain text in the file.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"changed"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_wrong_master_password_is_an_error() {
        let (path, cfg) = kdbx_config("wrongpw");
        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, true).unwrap();
        s.lock();

        let err = s.unlock(&cfg, "nope", true, false).unwrap_err();
        assert!(err.is(code::CRED_WRONG_PASSWORD), "unexpected: {:?}", err);
        assert!(!s.is_unlocked());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_locked_access_carries_the_marker() {
        let s = KeepassStore::default();
        let err = s.get("c1").unwrap_err();
        assert!(err.is(LOCKED), "unexpected: {:?}", err);
        assert!(s.set("c1", "x").unwrap_err().is(LOCKED));
        assert!(s.delete("c1").unwrap_err().is(LOCKED));
    }

    #[test]
    fn keepass_missing_file_needs_the_create_flag() {
        let (path, cfg) = kdbx_config("missing");
        let s = KeepassStore::default();
        let err = s.unlock(&cfg, "master", true, false).unwrap_err();
        assert!(err.is(code::CRED_NOT_FOUND), "unexpected: {:?}", err);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_one_shot_relocks_after_a_single_access() {
        let (path, cfg) = kdbx_config("oneshot");
        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, true).unwrap();
        s.set("c1", "hunter2").unwrap();
        s.lock();

        // "Do not remember": the session survives exactly one access.
        s.unlock(&cfg, "master", false, false).unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("hunter2"));
        assert!(!s.is_unlocked());
        assert!(s.get("c1").unwrap_err().is(LOCKED));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_finds_entries_outside_our_own_group() {
        let (path, cfg) = kdbx_config("interop");
        // Mimic KeePassXC: an entry sitting in a foreign group.
        let mut db = Database::new();
        {
            let mut root = db.root_mut();
            let mut foreign = root.add_group();
            foreign.name = "Internet".to_string();
            let mut entry = foreign.add_entry();
            entry.set_unprotected(fields::TITLE, "c1");
            entry.set_protected(fields::PASSWORD, "from-keepassxc");
        }
        write_atomic(&path, &db, database_key("master", None).unwrap()).unwrap();

        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, false).unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("from-keepassxc"));
        // An update stays where the user filed it.
        s.set("c1", "updated").unwrap();
        let reopened = Database::open(
            &mut std::fs::File::open(&path).unwrap(),
            database_key("master", None).unwrap(),
        )
        .unwrap();
        assert!(reopened.root().group_by_name(KP_GROUP).is_none());
        assert_eq!(
            reopened
                .root()
                .group_by_name("Internet")
                .unwrap()
                .entry_by_name("c1")
                .unwrap()
                .get_password(),
            Some("updated")
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_key_file_only_unlock() {
        let (path, mut cfg) = kdbx_config("keyfile");
        let key_file = path.parent().unwrap().join("secrets.keyx");
        std::fs::write(&key_file, b"not-a-password-just-bytes").unwrap();
        cfg.key_file = key_file.to_string_lossy().to_string();

        let s = KeepassStore::default();
        s.unlock(&cfg, "", true, true).unwrap();
        s.set("c1", "hunter2").unwrap();
        s.lock();
        s.unlock(&cfg, "", true, false).unwrap();
        assert_eq!(s.get("c1").unwrap().as_deref(), Some("hunter2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn keepass_needs_a_password_or_key_file() {
        let (path, cfg) = kdbx_config("emptykey");
        let s = KeepassStore::default();
        let err = s.unlock(&cfg, "", true, true).unwrap_err();
        assert!(err.is(code::CRED_WRONG_PASSWORD), "unexpected: {:?}", err);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Locates the KeePassXC CLI (PATH or the macOS app bundle).
    fn keepassxc_cli() -> Option<PathBuf> {
        let bundled = PathBuf::from("/Applications/KeePassXC.app/Contents/MacOS/keepassxc-cli");
        if bundled.exists() {
            return Some(bundled);
        }
        std::process::Command::new("keepassxc-cli")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|_| PathBuf::from("keepassxc-cli"))
    }

    /// Runs a keepassxc-cli subcommand, feeding `stdin` to its prompts.
    fn kpxc(cli: &Path, args: &[&str], stdin: &str) -> String {
        use std::io::Write as _;
        use std::process::{Command, Stdio};
        let mut child = Command::new(cli)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("keepassxc-cli startet");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "keepassxc-cli {:?} schlug fehl: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Live interoperability with KeePassXC (ticket 027). Both
    /// directions on a KDBX 4 file: KeePassXC reads what we wrote, and
    /// we read what KeePassXC wrote into the same database. Ignored by
    /// default (needs KeePassXC); run with `cargo test -- --ignored`.
    ///
    /// The database is created here rather than by `keepassxc-cli`
    /// because that CLI still writes the legacy KDBX 3.1 format — see
    /// `keepassxc_reads_kdbx3_but_we_do_not_write_it` for that path.
    #[test]
    #[ignore]
    fn keepassxc_interop_live() {
        let Some(cli) = keepassxc_cli() else {
            eprintln!("keepassxc-cli nicht gefunden — Test übersprungen");
            return;
        };
        let (path, cfg) = kdbx_config("kpxc");
        let db = path.to_string_lossy().to_string();

        // 1. hortcmd creates the KDBX 4 database and writes an entry.
        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, true).unwrap();
        s.set("from-hortcmd", "unser-geheimnis").unwrap();
        s.lock();

        // 2. KeePassXC opens it and reads our entry back.
        let ours = kpxc(
            &cli,
            &[
                "show",
                "-q",
                "-s",
                "-a",
                "Password",
                &db,
                "hortcmd/from-hortcmd",
            ],
            "master\n",
        );
        assert_eq!(ours.trim(), "unser-geheimnis");

        // 3. KeePassXC writes into the same file (entry + change).
        kpxc(
            &cli,
            &["add", "-q", "-p", &db, "from-kpxc"],
            "master\nfremdes-geheimnis\n",
        );
        kpxc(
            &cli,
            &["edit", "-q", "-p", &db, "hortcmd/from-hortcmd"],
            "master\nvon-kpxc-geaendert\n",
        );

        // 4. hortcmd reads both of those.
        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, false).unwrap();
        assert_eq!(
            s.get("from-kpxc").unwrap().as_deref(),
            Some("fremdes-geheimnis")
        );
        assert_eq!(
            s.get("from-hortcmd").unwrap().as_deref(),
            Some("von-kpxc-geaendert")
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// `keepassxc-cli db-create` still produces the legacy KDBX 3.1
    /// format. It stays readable here; a write names the upgrade rather
    /// than failing with a raw format error. Ignored like the test above.
    #[test]
    #[ignore]
    fn keepassxc_kdbx3_is_readable_but_not_writable() {
        let Some(cli) = keepassxc_cli() else {
            eprintln!("keepassxc-cli nicht gefunden — Test übersprungen");
            return;
        };
        let (path, cfg) = kdbx_config("kdbx3");
        let db = path.to_string_lossy().to_string();
        kpxc(&cli, &["db-create", "-q", "-p", &db], "master\nmaster\n");
        kpxc(
            &cli,
            &["add", "-q", "-p", &db, "alt"],
            "master\naltes-geheimnis\n",
        );

        let s = KeepassStore::default();
        s.unlock(&cfg, "master", true, false).unwrap();
        assert_eq!(s.get("alt").unwrap().as_deref(), Some("altes-geheimnis"));
        let err = s.set("neu", "x").unwrap_err();
        assert!(err.is(code::CRED_READ_ONLY_FORMAT), "unexpected: {:?}", err);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// The crate writes `None` metadata as empty XML elements, which
    /// KeePassXC's parser rejects. Guards `new_database()` on machines
    /// without KeePassXC, where `keepassxc_interop_live` cannot run.
    #[test]
    fn new_database_fills_the_metadata_keepassxc_requires() {
        let db = new_database();
        assert!(db.meta.memory_protection.is_some());
        assert!(db.meta.maintenance_history_days.is_some());
        assert!(db.meta.master_key_change_rec.is_some());
        assert!(db.meta.master_key_change_force.is_some());
        assert!(db.meta.recyclebin_enabled.is_some());
        assert!(db.meta.recyclebin_uuid.is_some());
        assert!(db.meta.entry_templates_group.is_some());
        assert!(db.meta.last_selected_group.is_some());
        assert!(db.meta.last_top_visible_group.is_some());
        assert!(db.meta.history_max_items.is_some());
        assert!(db.meta.history_max_size.is_some());
        let root = db.root();
        assert_eq!(root.icon(), Some(&keepass::db::Icon::BuiltIn(48)));
        assert!(root.enable_autotype.is_some());
        assert!(root.enable_searching.is_some());
    }

    #[test]
    fn config_defaults_to_the_keychain() {
        let cfg = CredConfig::default();
        assert_eq!(cfg.backend, "keychain");
        assert!(!cfg.is_keepass());
        // An unreadable/missing config file must not switch the backend.
        let parsed: CredConfig = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(parsed.backend, "keychain");
    }
}
