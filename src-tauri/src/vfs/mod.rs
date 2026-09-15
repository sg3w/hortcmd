// ============================================================
// Virtual file system (VFS) abstraction.
//
// A `Provider` trait hides all file-system operations behind a
// uniform interface. A scheme registry maps an address scheme
// (`local`, `sftp`, later `smb`/`webdav`/`ftp`) to its provider, so
// the commands in `commands/fs/*` operate schema-agnostically —
// the basis for the "identical UI" across local and remote
// locations (Concept.md §10 P4, ticket series 020–026).
//
// Local providers are stateless singletons; remote providers are
// bound to a live connection (see `net::session`) and are therefore
// resolved per address into an `Arc<dyn Provider>`.
// ============================================================

pub mod caps;
pub mod ftp;
pub mod identity;
pub mod limits;
pub mod local;
mod local_path;
pub mod location;
#[cfg(test)]
mod location_tests;
mod provider_path;
pub mod stream_path;
pub use stream_path::StreamPath;
pub mod sftp;
pub mod smb;
pub mod webdav;

// Live protocol tests against real servers (ignored by default).
#[cfg(test)]
mod harness;

use crate::commands::fs::dir::DirListing;
use crate::errors::AppResult;
use local::LocalProvider;
use std::io::{self, Read, Write};

use std::sync::{Arc, OnceLock};

/// Address scheme. `Local` and `Sftp` are implemented; the remaining
/// variants are reserved for upcoming providers and are parsed but
/// not yet resolvable.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Scheme {
    Local,
    Sftp,
    Smb,
    Webdav,
    Ftp,
}

impl Scheme {
    /// Canonical scheme name (as used in a URL-like address).
    pub fn as_str(&self) -> &'static str {
        match self {
            Scheme::Local => "local",
            Scheme::Sftp => "sftp",
            Scheme::Smb => "smb",
            Scheme::Webdav => "webdav",
            Scheme::Ftp => "ftp",
        }
    }

    /// Maps a scheme string to its variant. `file` and `local` both
    /// denote the local file system; unknown schemes return `None`.
    pub(crate) fn from_scheme_str(s: &str) -> Option<Scheme> {
        match s {
            "file" | "local" => Some(Scheme::Local),
            "sftp" => Some(Scheme::Sftp),
            "smb" => Some(Scheme::Smb),
            "webdav" => Some(Scheme::Webdav),
            "ftp" => Some(Scheme::Ftp),
            _ => None,
        }
    }

    /// Default port for a remote scheme (used when the address omits it).
    pub fn default_port(&self) -> Option<u16> {
        match self {
            Scheme::Sftp => Some(22),
            Scheme::Ftp => Some(21),
            Scheme::Smb => Some(445),
            Scheme::Webdav => Some(443),
            Scheme::Local => None,
        }
    }
}

/// Legacy command-boundary name for the versioned, typed location.
pub use location::VfsLocation as Address;

/// URI input boundary for existing string-valued Tauri/MCP commands.
pub fn parse_address(raw: &str) -> AppResult<Address> {
    location::VfsLocation::parse(raw)
}

/// The shared, stateless local provider.
fn local_provider() -> Arc<dyn Provider> {
    static LOCAL: OnceLock<Arc<LocalProvider>> = OnceLock::new();
    LOCAL.get_or_init(|| Arc::new(LocalProvider)).clone()
}

/// Resolves a raw address to its provider and provider-local path.
/// Local addresses use the shared local provider; `sftp` addresses are
/// bound to an active connection (see `net::session`). Unknown and
/// reserved-but-unimplemented schemes return a clear error.
pub fn resolve(raw: &str) -> AppResult<(Arc<dyn Provider>, StreamPath)> {
    resolve_location(&location::VfsLocation::parse(raw)?)
}

/// Resolve the provider and retain local/native or remote/component path identity.
pub fn resolve_location(
    location: &location::VfsLocation,
) -> AppResult<(Arc<dyn Provider>, StreamPath)> {
    location.validate()?;
    match location.kind() {
        location::LocationPath::Local { path } => {
            Ok((local_provider(), StreamPath::from(path.raw.clone())))
        }
        location::LocationPath::Remote {
            scheme,
            authority,
            path,
        } => {
            let provider =
                crate::net::session::remote_provider(scheme.as_str(), &authority.key(*scheme))?;
            Ok((provider, StreamPath::Remote(path.clone())))
        }
    }
}

/// Lightweight metadata subset used by the transfer engine.
pub struct Stat {
    pub is_dir: bool,
    pub len: u64,
}

/// Uniform file-system interface. Every provider (local, sftp, …)
/// implements these operations; the commands call only through
/// [`resolve`], never `std::fs` directly.
///
/// The nine core operations from ticket 020 map as follows: `list`,
/// `stat`, `read` = [`open_read`], `write` = [`write`] /
/// [`create_write`], `mkdir` = [`create_dir_all`], `rename`,
/// `delete` = [`remove`]. **copy** and **move** are built from the
/// streaming primitives `open_read` / `create_write` plus `rename` /
/// `remove` (see the transfer engine in `commands/fs/file.rs`), so
/// progress, pause, cancel and checksums work uniformly and — since
/// the engine holds a source *and* a destination provider — across
/// providers (local↔remote, remote↔remote).
#[derive(Debug, PartialEq, Eq)]
pub enum Health {
    Alive,
    Busy,
    Unsupported,
}

pub trait Provider: Send + Sync {
    /// Identity of the underlying store: `"local"` for the local file
    /// system, `"sftp:user@host:port"` for a remote connection. The
    /// transfer engine only takes the in-place `rename` fast-path when
    /// source and destination share a `store_id`.
    fn store_id(&self) -> String;

    /// Full directory listing (drives the `list_dir` command).
    fn list(&self, path: &str) -> AppResult<DirListing>;

    /// Cooperative cancellation for presentation reads. Protocol adapters whose
    /// API cannot interrupt a request still decline its abandoned result.
    fn list_cancellable(
        &self,
        path: &str,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> AppResult<DirListing> {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::errors::AppError::new(crate::errors::code::CANCELLED));
        }
        let result = self.list(path)?;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::errors::AppError::new(crate::errors::code::CANCELLED));
        }
        Ok(result)
    }

    /// Cheap metadata (following symlinks) for the transfer totals.
    fn stat(&self, path: &StreamPath) -> io::Result<Stat>;

    /// Whether the path exists.
    fn exists(&self, path: &StreamPath) -> bool;

    /// Whether the path is a directory (following symlinks).
    fn is_dir(&self, path: &StreamPath) -> bool;

    /// Child paths of a directory (errors propagate).
    fn read_dir(&self, path: &StreamPath) -> io::Result<Vec<StreamPath>>;

    /// Bounded traversal hook. Streaming providers can stop without materializing
    /// the entire directory. Legacy protocol APIs still have their own read bound.
    fn visit_dir(
        &self,
        path: &StreamPath,
        limit: usize,
        visit: &mut dyn FnMut(StreamPath) -> io::Result<()>,
    ) -> io::Result<()> {
        let entries = self.read_dir(path)?;
        if entries.len() > limit {
            return Err(io::Error::other("preflight directory limit"));
        }
        for entry in entries {
            visit(entry)?;
        }
        Ok(())
    }

    fn limits(&self, path: &StreamPath) -> io::Result<limits::ProviderLimits> {
        Ok(limits::ProviderLimits {
            available_bytes: self.free_space(path),
            staging: self.supports_staging(),
            ..Default::default()
        })
    }

    /// A cheap stable object/version token where supported. Never a cached answer.
    fn planning_identity(&self, _path: &StreamPath) -> io::Result<Option<String>> {
        Ok(None)
    }

    /// Opens a file for streaming reads (transfer / checksum).
    fn open_read(&self, path: &StreamPath) -> io::Result<Box<dyn Read + Send>>;

    /// Creates/truncates a file for streaming writes (transfer).
    fn create_write(&self, path: &StreamPath) -> io::Result<Box<dyn Write + Send>>;

    /// Writes a whole file at once (small helper for text export).
    fn write(&self, path: &StreamPath, data: &[u8]) -> io::Result<()>;

    /// Creates a directory and all missing parents (mkdir).
    fn create_dir_all(&self, path: &StreamPath) -> io::Result<()>;

    /// Renames/moves an entry within the same provider.
    fn rename(&self, from: &StreamPath, to: &StreamPath) -> io::Result<()>;

    /// Removes a single file.
    fn remove_file(&self, path: &StreamPath) -> io::Result<()>;

    /// Removes a directory and its contents recursively.
    fn remove_dir_all(&self, path: &StreamPath) -> io::Result<()>;

    /// Whether this provider's transport is still usable.
    ///
    /// Answered from local state only — never a round trip — because the
    /// connection status refresh asks every live provider (ticket 032).
    /// A provider that cannot tell cheaply says `true` and reports a
    /// loss from its own operations instead; a definitive `false` makes
    /// the session disconnect once rather than staying marked live.
    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        None
    }

    /// Authenticated, read-only health check. Must bound its own I/O and
    /// coordinate shared channels; unavailable adapters say so explicitly.
    fn health_check(&self, _timeout: std::time::Duration) -> io::Result<Health> {
        Ok(Health::Unsupported)
    }

    fn transport_alive(&self) -> bool {
        true
    }

    /// The SSH connection carrying this provider, if it has one (SFTP).
    /// The SSH terminal (ticket 026) opens its shell channel on it, so
    /// no second login is needed; every other provider returns `None`.
    fn ssh(&self) -> Option<Arc<crate::net::ssh::SshSession>> {
        None
    }

    // ----- Failure-safe transfers (ticket 029) -----

    /// Whether a staged entry can be published under its final name with
    /// [`rename`](Provider::rename) inside this store. Safe transfers
    /// write next to the target and commit with one rename, so a
    /// provider without dependable rename semantics declares `false`
    /// here — the transfer then refuses instead of quietly writing
    /// straight to the final name.
    fn supports_staging(&self) -> bool {
        false
    }

    /// Whether [`rename`](Provider::rename) replaces an existing **file**
    /// target atomically. Where it does, a replacement never leaves a
    /// moment without a destination; where it does not, the transfer
    /// engine renames the old destination aside first and puts it back
    /// if the commit fails. Directory targets always take the backup
    /// route — `rename` onto a populated directory is not portable.
    fn rename_replaces(&self) -> bool {
        false
    }

    /// Free bytes on the store carrying `path`, when the provider can
    /// state it definitively. `None` means unknown: staging proceeds and
    /// a genuine write failure surfaces normally, rather than blocking a
    /// transfer on a guess.
    fn free_space(&self, _path: &StreamPath) -> Option<u64> {
        None
    }

    /// Removes an entry regardless of kind (file or directory).
    fn remove(&self, path: &StreamPath) -> io::Result<()> {
        if self.is_dir(path) {
            self.remove_dir_all(path)
        } else {
            self.remove_file(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_path_is_local() {
        let a = parse_address("/Users/x/file.txt").unwrap();
        assert_eq!(a.scheme(), Scheme::Local);
        assert_eq!(a.path(), "/Users/x/file.txt");
        assert_eq!(a.connection_key(), None);
    }

    #[test]
    fn relative_path_is_local() {
        let a = parse_address("some/rel path").unwrap();
        assert_eq!(a.scheme(), Scheme::Local);
        assert_eq!(a.path(), "some/rel path");
    }

    #[test]
    fn file_url_maps_to_local_path() {
        let a = parse_address("file:///Users/x").unwrap();
        assert_eq!(a.scheme(), Scheme::Local);
        assert_eq!(a.path(), "/Users/x");
    }

    #[test]
    fn sftp_authority_is_parsed() {
        let a = parse_address("sftp://alice@host.example:2222/srv/data").unwrap();
        assert_eq!(a.scheme(), Scheme::Sftp);
        assert_eq!(a.user(), Some("alice"));
        assert_eq!(a.host(), Some("host.example"));
        assert_eq!(a.port(), Some(2222));
        assert_eq!(a.path(), "/srv/data");
        assert_eq!(
            a.connection_key().as_deref(),
            Some("alice@host.example:2222")
        );
    }

    #[test]
    fn sftp_defaults_user_and_port() {
        let a = parse_address("sftp://host/only").unwrap();
        assert_eq!(a.user(), None);
        assert_eq!(a.host(), Some("host"));
        assert_eq!(a.port(), None);
        // Default port 22 fills in for the connection key.
        assert_eq!(a.connection_key().as_deref(), Some("@host:22"));
    }

    #[test]
    fn sftp_without_active_connection_errors() {
        // Parsed fine, but no session is registered → clear error.
        assert!(resolve("sftp://user@host/srv/data").is_err());
    }

    #[test]
    fn unknown_scheme_is_error() {
        assert!(parse_address("gopher://host/x").is_err());
        assert!(resolve("gopher://host/x").is_err());
    }

    #[test]
    fn all_remote_schemes_parse_with_defaults() {
        // Each remote scheme keeps its authority and fills in its
        // default port for the connection key.
        for (raw, scheme, key) in [
            (
                "ftp://bob@files.example/pub",
                Scheme::Ftp,
                "bob@files.example:21",
            ),
            (
                "smb://u@nas.local/share/dir",
                Scheme::Smb,
                "u@nas.local:445",
            ),
            (
                "webdav://u@dav.example/remote",
                Scheme::Webdav,
                "u@dav.example:443",
            ),
        ] {
            let a = parse_address(raw).unwrap();
            assert_eq!(a.scheme(), scheme, "scheme of {}", raw);
            assert_eq!(a.connection_key().as_deref(), Some(key), "key of {}", raw);
        }
    }

    #[test]
    fn remote_schemes_without_connection_error() {
        // Parsed fine, but nothing is connected → clear error, never a
        // silent fallback to the local file system.
        for raw in [
            "ftp://bob@files.example/pub",
            "smb://u@nas.local/share",
            "webdav://u@dav.example/remote",
        ] {
            assert!(resolve(raw).is_err(), "{} should not resolve", raw);
        }
    }

    #[test]
    fn local_address_resolves_to_local_provider() {
        let (p, path) = resolve("/tmp/x").unwrap();
        assert_eq!(p.store_id(), "local");
        assert_eq!(path, StreamPath::from("/tmp/x"));
    }
}
