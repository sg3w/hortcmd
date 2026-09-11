// ============================================================
// Networking base for the remote VFS providers (ticket 021).
//
//   credentials – CredentialStore trait + OS-keychain backend, plus the
//                 optional portable KeePass (.kdbx) backend (ticket 027)
//   hostkey     – SSH host-key verification against known_hosts plus the
//                 hortcmd-managed trust store (ticket 028)
//   lifecycle   – attempt/generation state machine behind connecting:
//                 one attempt per profile, bounded, cancellable, and the
//                 single gate a provider passes to become live (032)
//   session     – connection profiles + live session registry
//                 (connect/disconnect/reconnect), protocol-independent
//   ssh         – authenticated SSH connection (SFTP + terminal ride on it)
//   terminal    – interactive SSH shell sessions (ticket 026)
// ============================================================

pub mod credentials;
pub mod hostkey;
pub mod lifecycle;
pub mod session;
pub mod ssh;
pub mod terminal;

/// Small unique id (no extra dependency): time plus a process-wide
/// counter. Used for terminal sessions and host-key prompt tokens.
pub fn unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", nanos, N.fetch_add(1, Ordering::Relaxed))
}
