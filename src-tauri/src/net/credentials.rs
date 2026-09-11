// ============================================================
// Credential storage (ticket 021).
//
// Secrets (passwords / key passphrases) are kept in the OS keychain,
// never in plain text. The `CredentialStore` trait keeps the callers
// backend-agnostic so a portable backend (e.g. KeePass, ticket 027)
// can be slotted in later without changes at the call sites.
//
// The default backend is `KeychainStore` (the `keyring` crate):
// macOS Keychain, Windows Credential Manager, Linux Secret Service.
// A missing Secret Service (headless Linux) is reported as a clear
// error — there is deliberately **no** silent plain-text fallback.
// ============================================================

use keyring::Entry;

/// Keychain service name under which all connection secrets are filed.
const SERVICE: &str = "com.hortcmd.app";

/// Backend-agnostic secret storage keyed by connection id.
pub trait CredentialStore: Send + Sync {
    /// Returns the stored secret, or `None` if none is stored.
    /// A backend/access failure is an `Err`, never a silent `None`.
    fn get(&self, id: &str) -> Result<Option<String>, String>;

    /// Stores (or replaces) the secret for a connection id.
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;

    /// Removes the secret; removing a missing entry is not an error.
    fn delete(&self, id: &str) -> Result<(), String>;
}

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

    fn entry(&self, id: &str) -> Result<Entry, String> {
        Entry::new(&self.service, id).map_err(explain)
    }
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeychainStore {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match self.entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(explain(e)),
        }
    }

    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        self.entry(id)?.set_password(secret).map_err(explain)
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        match self.entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(explain(e)),
        }
    }
}

/// Turns a keyring error into a user-facing message. On Linux a
/// missing/locked Secret Service is the common failure, so the remedy
/// is spelled out instead of leaking a raw platform error.
fn explain(e: keyring::Error) -> String {
    match e {
        keyring::Error::NoStorageAccess(inner) => no_service_hint(&inner.to_string()),
        keyring::Error::PlatformFailure(inner) => no_service_hint(&inner.to_string()),
        other => format!("Schlüsselbund-Fehler: {}", other),
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory store for deterministic tests (no OS keychain).
    #[derive(Default)]
    pub(crate) struct MemoryStore {
        map: Mutex<HashMap<String, String>>,
    }

    impl CredentialStore for MemoryStore {
        fn get(&self, id: &str) -> Result<Option<String>, String> {
            Ok(self.map.lock().unwrap().get(id).cloned())
        }
        fn set(&self, id: &str, secret: &str) -> Result<(), String> {
            self.map
                .lock()
                .unwrap()
                .insert(id.to_string(), secret.to_string());
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
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
}
