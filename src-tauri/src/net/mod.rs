// ============================================================
// Networking base for the remote VFS providers (ticket 021).
//
//   credentials – CredentialStore trait + OS-keychain backend
//   session     – connection profiles + live session registry
//                 (connect/disconnect/reconnect), protocol-independent
//   ssh         – authenticated SSH connection (SFTP + terminal ride on it)
//   terminal    – interactive SSH shell sessions (ticket 026)
// ============================================================

pub mod credentials;
pub mod session;
pub mod ssh;
pub mod terminal;
