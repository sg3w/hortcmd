// ============================================================
// SSH host-key verification (ticket 028).
//
// Every SSH connection must authenticate the *server* before a
// password, a key signature or an agent signature leaves the machine.
// The check runs inside the russh handshake (`net::ssh`), so SFTP and
// the terminal riding on the same session are covered by one place.
//
// Two trust sources are consulted, both in OpenSSH `known_hosts` form:
// the user's own files (read only — hortcmd never rewrites them) and a
// hortcmd-managed store in the app config directory, which is where
// keys accepted in the connection manager land. A conflicting entry in
// *either* source blocks the connection; a match in one source never
// overrides a conflict in the other.
// ============================================================

use russh::keys::{known_hosts::known_host_keys_path, HashAlg, PublicKey};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use ts_rs::TS;

// ---------- Host identity ----------

/// Canonical identity of an SSH endpoint. Keys are trusted per
/// host *and* port, so moving a profile to another endpoint never
/// inherits the previous host's trust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostId {
    pub host: String,
    pub port: u16,
}

impl HostId {
    /// Normalizes what a profile stores (which may carry a scheme, a
    /// trailing slash or a bracketed IPv6 literal) into the bare host.
    pub fn new(host: &str, port: u16) -> Self {
        let host = host.split_once("://").map(|(_, rest)| rest).unwrap_or(host);
        let host = host.trim().trim_end_matches('/');
        let host = host
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']').map(|(inner, _)| inner))
            .unwrap_or(host);
        HostId {
            host: host.to_lowercase(),
            port,
        }
    }

    /// OpenSSH identity of the endpoint: the bare host on the default
    /// port, `[host]:port` on every other port.
    pub fn identity(&self) -> String {
        if self.port == DEFAULT_SSH_PORT {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        }
    }
}

const DEFAULT_SSH_PORT: u16 = 22;

// ---------- IPC types ----------

/// A key already trusted for a host, plus where that entry lives — the
/// mismatch dialog names the file the user has to look at.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StoredHostKey {
    pub source: String,
    #[ts(type = "number")]
    pub line: u32,
    pub key_type: String,
    /// `SHA256:…`, the format OpenSSH prints.
    pub fingerprint: String,
}

/// A host-key decision the user has to make. Returned *instead of* a
/// connection, before any authentication has been attempted.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HostKeyChallenge {
    /// `"unknown"` (first contact) or `"mismatch"` (stored key differs).
    pub status: String,
    /// One-shot token authorising trust for exactly this key. `null` on
    /// a mismatch — replacing a trusted key is a separate, explicit
    /// action, never a button in the failure dialog.
    pub token: Option<String>,
    pub host: String,
    #[ts(type = "number")]
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    /// The conflicting entries; empty unless `status == "mismatch"`.
    pub stored: Vec<StoredHostKey>,
}

/// Outcome of dropping a host's keys from the hortcmd trust store.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ForgetResult {
    #[ts(type = "number")]
    pub removed: u32,
    /// User-owned `known_hosts` files that still hold entries for the
    /// host. hortcmd does not rewrite them — the user has to.
    pub remaining: Vec<String>,
}

// ---------- Connection outcome ----------

/// Why a connection attempt produced no live session. Typed so the
/// frontend picks its UI from the variant, never from message text.
#[derive(Debug)]
pub enum ConnectError {
    /// The server identity is unknown or has changed. Nothing was
    /// authenticated and no secret was sent.
    HostKey(HostKeyChallenge),
    /// Anything else, already phrased for the user.
    Message(String),
}

impl ConnectError {
    /// Flattens to a message for callers without a host-key UI.
    pub fn message(&self) -> String {
        match self {
            ConnectError::Message(m) => m.clone(),
            ConnectError::HostKey(c) if c.status == "mismatch" => format!(
                "Host-Schlüssel von {}:{} hat sich geändert ({} {}) — Verbindung abgelehnt",
                c.host, c.port, c.key_type, c.fingerprint
            ),
            ConnectError::HostKey(c) => format!(
                "Host-Schlüssel von {}:{} ist unbekannt ({} {}) — bitte im Verbindungsmanager bestätigen",
                c.host, c.port, c.key_type, c.fingerprint
            ),
        }
    }
}

impl From<String> for ConnectError {
    fn from(message: String) -> Self {
        ConnectError::Message(message)
    }
}

impl From<&str> for ConnectError {
    fn from(message: &str) -> Self {
        ConnectError::Message(message.to_string())
    }
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// What the handshake decided about the presented key.
pub enum Decision {
    Trusted,
    /// The user has to answer before authentication may start.
    Challenge(HostKeyChallenge),
}

// ---------- Fingerprints ----------

/// `SHA256:…` fingerprint, identical to what `ssh-keygen -lf` prints.
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn key_type(key: &PublicKey) -> String {
    key.algorithm().as_str().to_string()
}

// ---------- Trust sources ----------

/// The hortcmd-managed store — keys accepted in the connection manager.
fn app_store_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Kein Konfigurationsverzeichnis gefunden".to_string())?
        .join("hortcmd");
    Ok(dir.join("known_hosts"))
}

/// The user's own OpenSSH files. Read only — never rewritten.
fn user_store_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    vec![ssh.join("known_hosts"), ssh.join("known_hosts2")]
}

/// Every file consulted during verification, managed store first.
fn sources() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(app) = app_store_path() {
        paths.push(app);
    }
    paths.extend(user_store_paths());
    paths
}

/// Reads the entries of `path` that apply to `id`. A file that does not
/// exist simply contributes nothing.
fn entries_for(id: &HostId, path: &Path) -> Result<Vec<(usize, PublicKey)>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    known_host_keys_path(&id.host, id.port, path).map_err(|e| unreadable(path, &e.to_string()))
}

/// Describes an unreadable known-hosts file, naming the offending lines
/// where they can be pinned down. Entries for *other* hosts stay
/// unaffected — only a broken entry for this host reaches here.
fn unreadable(path: &Path, reason: &str) -> String {
    let lines = malformed_lines(path);
    if lines.is_empty() {
        return format!(
            "{} konnte nicht gelesen werden: {}",
            path.display(),
            reason
        );
    }
    let list: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    format!(
        "{}: unlesbarer known_hosts-Eintrag in Zeile {} ({})",
        path.display(),
        list.join(", "),
        reason
    )
}

/// Best-effort scan for lines whose key blob does not parse.
fn malformed_lines(path: &Path) -> Vec<usize> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
                return None;
            }
            let blob = line.split(' ').nth(2)?;
            russh::keys::parse_public_key_base64(blob)
                .err()
                .map(|_| index + 1)
        })
        .collect()
}

// ---------- Verification ----------

/// Verifies `presented` against `paths`. Returns whether a matching
/// entry exists and every conflicting entry found along the way.
fn scan(
    id: &HostId,
    presented: &PublicKey,
    paths: &[PathBuf],
) -> Result<(bool, Vec<StoredHostKey>), String> {
    let mut matched = false;
    let mut conflicts = Vec::new();
    for path in paths {
        for (line, stored) in entries_for(id, path)? {
            if stored == *presented {
                // Duplicates across or inside files are harmless.
                matched = true;
            } else if stored.algorithm() == presented.algorithm() {
                // Same algorithm, different key — that is a changed host
                // key, not a second key type the host also offers.
                conflicts.push(StoredHostKey {
                    source: path.display().to_string(),
                    line: line as u32,
                    key_type: key_type(&stored),
                    fingerprint: fingerprint(&stored),
                });
            }
        }
    }
    Ok((matched, conflicts))
}

fn challenge(
    status: &str,
    token: Option<String>,
    id: &HostId,
    presented: &PublicKey,
    stored: Vec<StoredHostKey>,
) -> HostKeyChallenge {
    HostKeyChallenge {
        status: status.to_string(),
        token,
        host: id.host.clone(),
        port: id.port,
        key_type: key_type(presented),
        fingerprint: fingerprint(presented),
        stored,
    }
}

/// Decides whether the handshake may continue. A conflict anywhere wins
/// over a match anywhere; only an untouched, matching host is trusted.
pub fn check(id: &HostId, presented: &PublicKey) -> Result<Decision, String> {
    let (matched, conflicts) = scan(id, presented, &sources())?;
    if !conflicts.is_empty() {
        return Ok(Decision::Challenge(challenge(
            "mismatch", None, id, presented, conflicts,
        )));
    }
    if matched {
        return Ok(Decision::Trusted);
    }
    let token = remember(id, presented);
    Ok(Decision::Challenge(challenge(
        "unknown",
        Some(token),
        id,
        presented,
        Vec::new(),
    )))
}

// ---------- Pending decisions ----------

/// The key shown to the user, kept until they answer. Keeping it here
/// rather than round-tripping it through the frontend means a "trust"
/// call can only ever confirm the key hortcmd actually displayed.
struct Pending {
    id: HostId,
    key: PublicKey,
    token: String,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parks a key under a fresh token. One entry per host keeps the map
/// bounded when a prompt is dismissed instead of answered.
fn remember(id: &HostId, key: &PublicKey) -> String {
    let token = super::unique_id();
    pending().lock().unwrap().insert(
        id.identity(),
        Pending {
            id: id.clone(),
            key: key.clone(),
            token: token.clone(),
        },
    );
    token
}

fn take(token: &str) -> Option<Pending> {
    let mut guard = pending().lock().unwrap();
    let identity = guard
        .iter()
        .find(|(_, p)| p.token == token)
        .map(|(identity, _)| identity.clone())?;
    guard.remove(&identity)
}

// ---------- Trust store ----------

/// Persists the key behind `token`. The token is consumed, so a second
/// call cannot re-trust it, and a failed write aborts the whole flow.
pub fn trust(token: &str) -> Result<HostId, String> {
    let pending = take(token).ok_or_else(|| {
        "Die Host-Schlüssel-Abfrage ist abgelaufen — bitte erneut verbinden".to_string()
    })?;
    append(&app_store_path()?, &pending.id, &pending.key)?;
    Ok(pending.id)
}

/// Records a key for `id` in the managed store — how the live host-key
/// test stages a rotated key. The production path goes through `trust`,
/// which can only confirm a key hortcmd actually displayed.
#[cfg(test)]
pub(crate) fn record(id: &HostId, key: &PublicKey) -> Result<(), String> {
    append(&app_store_path()?, id, key)
}

/// Appends one OpenSSH-formatted entry to the managed store.
fn append(path: &Path, id: &HostId, key: &PublicKey) -> Result<(), String> {
    let openssh = key
        .to_openssh()
        .map_err(|e| format!("Host-Schlüssel konnte nicht kodiert werden: {}", e))?;
    let mut content = std::fs::read_to_string(path).unwrap_or_default();
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{} {}\n", id.identity(), openssh.trim()));
    write_atomic(path, &content)
}

/// Removes every managed entry for `id`. The user's own files are only
/// inspected, so a conflict there is reported rather than edited away.
pub fn forget(id: &HostId) -> Result<ForgetResult, String> {
    let store = app_store_path()?;
    let removed = remove_from(&store, id)?;
    let mut remaining = Vec::new();
    for path in user_store_paths() {
        if !entries_for(id, &path).unwrap_or_default().is_empty() {
            remaining.push(path.display().to_string());
        }
    }
    Ok(ForgetResult { removed, remaining })
}

/// Drops the lines of `path` whose host field is exactly `id`. Only the
/// managed store is passed here, and hortcmd writes it in plain
/// (unhashed, single-host) form, so a literal comparison is enough.
fn remove_from(path: &Path, id: &HostId) -> Result<u32, String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(0);
    };
    let identity = id.identity();
    let mut removed = 0;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            if line.split(' ').next() == Some(identity.as_str()) {
                removed += 1;
                return false;
            }
            true
        })
        .collect();
    if removed > 0 {
        let mut content = kept.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        write_atomic(path, &content)?;
    }
    Ok(removed)
}

/// Writes through a temporary sibling and renames it into place, so a
/// crash never leaves a half-written trust store behind.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("hortcmd-tmp");
    let write = || -> std::io::Result<()> {
        std::fs::write(&tmp, content)?;
        restrict(&tmp)
    };
    if let Err(e) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "{} konnte nicht geschrieben werden: {}",
            path.display(),
            e
        ));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{} konnte nicht ersetzt werden: {}", path.display(), e)
    })
}

/// Owner-only permissions for the trust store on Unix; on Windows the
/// per-user config directory already provides that.
#[cfg(unix)]
fn restrict(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::parse_public_key_base64;

    /// Two distinct, valid ed25519 host keys.
    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

    fn key(blob: &str) -> PublicKey {
        parse_public_key_base64(blob).expect("test key")
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hortcmd-hostkey-{}-{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("known_hosts")
    }

    #[test]
    fn identity_uses_the_openssh_port_form() {
        assert_eq!(HostId::new("example.com", 22).identity(), "example.com");
        assert_eq!(
            HostId::new("example.com", 2222).identity(),
            "[example.com]:2222"
        );
        assert_eq!(HostId::new("10.0.0.5", 22).identity(), "10.0.0.5");
        assert_eq!(HostId::new("::1", 22).identity(), "::1");
        assert_eq!(HostId::new("[::1]", 2222).identity(), "[::1]:2222");
    }

    #[test]
    fn identity_normalizes_what_a_profile_stores() {
        assert_eq!(HostId::new("sftp://Example.COM/", 22).host, "example.com");
        assert_eq!(HostId::new("  example.com  ", 22).host, "example.com");
    }

    #[test]
    fn a_changed_port_is_a_different_identity() {
        assert_ne!(
            HostId::new("example.com", 22).identity(),
            HostId::new("example.com", 2200).identity()
        );
    }

    #[test]
    fn fingerprints_are_stable_sha256() {
        let fp = fingerprint(&key(KEY_A));
        assert!(fp.starts_with("SHA256:"), "unexpected format: {}", fp);
        assert_eq!(fp, fingerprint(&key(KEY_A)));
        assert_ne!(fp, fingerprint(&key(KEY_B)));
    }

    #[test]
    fn a_missing_file_leaves_the_host_unknown() {
        let path = scratch("missing").with_file_name("nothing-here");
        let id = HostId::new("example.com", 22);
        let (matched, conflicts) = scan(&id, &key(KEY_A), &[path]).unwrap();
        assert!(!matched);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_matching_entry_is_trusted_and_duplicates_are_harmless() {
        let path = scratch("match");
        let id = HostId::new("example.com", 2222);
        append(&path, &id, &key(KEY_A)).unwrap();
        append(&path, &id, &key(KEY_A)).unwrap();
        let (matched, conflicts) = scan(&id, &key(KEY_A), &[path]).unwrap();
        assert!(matched);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_different_key_for_the_same_host_is_a_conflict() {
        let path = scratch("conflict");
        let id = HostId::new("example.com", 22);
        append(&path, &id, &key(KEY_A)).unwrap();
        let (matched, conflicts) = scan(&id, &key(KEY_B), &[path]).unwrap();
        assert!(!matched);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].fingerprint, fingerprint(&key(KEY_A)));
        assert_eq!(conflicts[0].line, 1);
    }

    #[test]
    fn trust_for_one_port_does_not_cover_another() {
        let path = scratch("ports");
        append(&path, &HostId::new("example.com", 22), &key(KEY_A)).unwrap();
        let other = HostId::new("example.com", 2222);
        let (matched, conflicts) = scan(&other, &key(KEY_A), &[path]).unwrap();
        assert!(!matched);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn a_conflict_in_one_source_outweighs_a_match_in_another() {
        let good = scratch("two-good");
        let bad = good.with_file_name("known_hosts_other");
        let id = HostId::new("example.com", 22);
        append(&good, &id, &key(KEY_A)).unwrap();
        append(&bad, &id, &key(KEY_B)).unwrap();
        let (matched, conflicts) = scan(&id, &key(KEY_A), &[good, bad]).unwrap();
        assert!(matched);
        assert_eq!(conflicts.len(), 1, "the stale entry must still block");
    }

    #[test]
    fn a_malformed_entry_names_its_file_and_line() {
        let path = scratch("broken");
        std::fs::write(&path, "example.com ssh-ed25519 not-a-key\n").unwrap();
        let id = HostId::new("example.com", 22);
        let err = scan(&id, &key(KEY_A), &[path.clone()]).unwrap_err();
        assert!(err.contains(&path.display().to_string()), "{}", err);
        assert!(err.contains("Zeile 1"), "{}", err);
    }

    #[test]
    fn a_malformed_entry_for_another_host_does_not_disable_verification() {
        let path = scratch("broken-other");
        std::fs::write(&path, "other.example ssh-ed25519 not-a-key\n").unwrap();
        let id = HostId::new("example.com", 22);
        append(&path, &id, &key(KEY_A)).unwrap();
        let (matched, conflicts) = scan(&id, &key(KEY_A), &[path]).unwrap();
        assert!(matched);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn comments_and_hashed_entries_are_understood() {
        let path = scratch("hashed");
        // Comment line, then a hashed entry for example.com:22 holding
        // a third key — taken from the OpenSSH-compatible fixtures.
        std::fs::write(
            &path,
            "#example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G2sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X\n\
             |1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF\n",
        )
        .unwrap();
        let id = HostId::new("example.com", 22);
        let hashed = key("AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF");
        let (matched, conflicts) = scan(&id, &hashed, &[path]).unwrap();
        assert!(matched, "hashed hostnames must be matched");
        assert!(conflicts.is_empty(), "the commented-out key must be ignored");
    }

    #[test]
    fn removing_an_entry_leaves_the_other_hosts_alone() {
        let path = scratch("remove");
        let gone = HostId::new("example.com", 2222);
        let stays = HostId::new("other.example", 22);
        append(&path, &gone, &key(KEY_A)).unwrap();
        append(&path, &stays, &key(KEY_B)).unwrap();

        assert_eq!(remove_from(&path, &gone).unwrap(), 1);
        assert_eq!(remove_from(&path, &gone).unwrap(), 0);

        let (matched, _) = scan(&stays, &key(KEY_B), &[path.clone()]).unwrap();
        assert!(matched);
        let (matched, conflicts) = scan(&gone, &key(KEY_A), &[path]).unwrap();
        assert!(!matched);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn an_unknown_token_cannot_trust_anything() {
        assert!(trust("does-not-exist").is_err());
    }

    #[test]
    fn a_token_works_once() {
        let id = HostId::new("token.example", 22);
        let token = remember(&id, &key(KEY_A));
        assert!(take(&token).is_some());
        assert!(take(&token).is_none());
    }
}
