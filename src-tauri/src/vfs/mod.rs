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
pub mod sftp;
pub mod smb;
pub mod webdav;

// Live protocol tests against real servers (ignored by default).
#[cfg(test)]
mod harness;

use crate::commands::fs::dir::DirListing;
use crate::errors::{AppError, AppResult};
use local::LocalProvider;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Address scheme. `Local` and `Sftp` are implemented; the remaining
/// variants are reserved for upcoming providers and are parsed but
/// not yet resolvable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    fn from_scheme_str(s: &str) -> Option<Scheme> {
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

/// A parsed address: a scheme, an optional remote authority, and the
/// provider-local path. Plain local paths (no `scheme://`) map to
/// `Scheme::Local` with the path unchanged, so existing local paths
/// keep working verbatim.
#[derive(Clone, Debug)]
pub struct Address {
    pub scheme: Scheme,
    /// Remote user (from `user@host`), if any.
    pub user: Option<String>,
    /// Remote host, if any.
    pub host: Option<String>,
    /// Remote port (explicit `:port`), if any.
    pub port: Option<u16>,
    /// Path as seen by the provider. For `local` this is the raw
    /// file-system path; for a remote scheme it is the path portion
    /// after `scheme://[user@host[:port]]`.
    pub path: String,
}

impl Address {
    /// Stable identity of the connection an address points at
    /// (`user@host:port`); `None` for local addresses. Two addresses
    /// with the same key share a session/store.
    pub fn connection_key(&self) -> Option<String> {
        let host = self.host.as_ref()?;
        let port = self.port.or_else(|| self.scheme.default_port());
        let user = self.user.clone().unwrap_or_default();
        Some(match port {
            Some(p) => format!("{}@{}:{}", user, host, p),
            None => format!("{}@{}", user, host),
        })
    }
}

/// Parses a raw address string into a scheme, optional authority, and
/// provider-local path.
///
/// - No `scheme://` prefix → a local path, used verbatim.
/// - `file://…` / `local://…` → local path (the leading slashes of an
///   absolute `file:///…` URL stay part of the path).
/// - `sftp://[user@]host[:port]/path` → remote scheme + authority.
/// - Any other scheme → an error.
pub fn parse_address(raw: &str) -> AppResult<Address> {
    let Some(idx) = raw.find("://") else {
        // No scheme: a plain local path.
        return Ok(local_address(raw));
    };

    let scheme_str = &raw[..idx];
    let rest = &raw[idx + 3..];
    match Scheme::from_scheme_str(scheme_str) {
        Some(Scheme::Local) => Ok(local_address(rest)),
        Some(scheme) => Ok(parse_remote(scheme, rest)),
        None => Err(AppError::from(format!(
            "Unbekanntes Adressschema: {}://",
            scheme_str
        ))),
    }
}

fn local_address(path: &str) -> Address {
    Address {
        scheme: Scheme::Local,
        user: None,
        host: None,
        port: None,
        path: path.to_string(),
    }
}

/// Splits a remote address body `user@host:port/path` into parts.
/// A missing path defaults to `/`.
fn parse_remote(scheme: Scheme, rest: &str) -> Address {
    let (authority, path) = match rest.find('/') {
        Some(pos) => (&rest[..pos], rest[pos..].to_string()),
        None => (rest, "/".to_string()),
    };

    let (user, host_port) = match authority.rsplit_once('@') {
        Some((u, hp)) => (Some(u.to_string()), hp),
        None => (None, authority),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()),
        None => (host_port.to_string(), None),
    };

    Address {
        scheme,
        user,
        host: (!host.is_empty()).then_some(host),
        port,
        path,
    }
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
pub fn resolve(raw: &str) -> AppResult<(Arc<dyn Provider>, String)> {
    let addr = parse_address(raw)?;
    if addr.scheme == Scheme::Local {
        return Ok((local_provider(), addr.path));
    }
    // Every remote scheme is served by a live connection from the
    // session registry (opened via the connection manager, ticket 021).
    let key = addr.connection_key().ok_or_else(|| {
        AppError::from(format!(
            "{}-Adresse ohne Host",
            addr.scheme.as_str().to_uppercase()
        ))
    })?;
    let provider = crate::net::session::remote_provider(addr.scheme.as_str(), &key)?;
    Ok((provider, addr.path))
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
pub trait Provider: Send + Sync {
    /// Identity of the underlying store: `"local"` for the local file
    /// system, `"sftp:user@host:port"` for a remote connection. The
    /// transfer engine only takes the in-place `rename` fast-path when
    /// source and destination share a `store_id`.
    fn store_id(&self) -> String;

    /// Full directory listing (drives the `list_dir` command).
    fn list(&self, path: &str) -> AppResult<DirListing>;

    /// Cheap metadata (following symlinks) for the transfer totals.
    fn stat(&self, path: &Path) -> io::Result<Stat>;

    /// Whether the path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Whether the path is a directory (following symlinks).
    fn is_dir(&self, path: &Path) -> bool;

    /// Child paths of a directory (errors propagate).
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;

    /// Bounded traversal hook. Streaming providers can stop without materializing
    /// the entire directory. Legacy protocol APIs still have their own read bound.
    fn visit_dir(
        &self,
        path: &Path,
        limit: usize,
        visit: &mut dyn FnMut(PathBuf) -> io::Result<()>,
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

    fn limits(&self, path: &Path) -> io::Result<limits::ProviderLimits> {
        Ok(limits::ProviderLimits {
            available_bytes: self.free_space(path),
            staging: self.supports_staging(),
            ..Default::default()
        })
    }

    /// A cheap stable object/version token where supported. Never a cached answer.
    fn planning_identity(&self, _path: &Path) -> io::Result<Option<String>> {
        Ok(None)
    }

    /// Opens a file for streaming reads (transfer / checksum).
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>>;

    /// Creates/truncates a file for streaming writes (transfer).
    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>>;

    /// Writes a whole file at once (small helper for text export).
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;

    /// Creates a directory and all missing parents (mkdir).
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;

    /// Renames/moves an entry within the same provider.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Removes a single file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Removes a directory and its contents recursively.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;

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
    fn free_space(&self, _path: &Path) -> Option<u64> {
        None
    }

    /// Removes an entry regardless of kind (file or directory).
    fn remove(&self, path: &Path) -> io::Result<()> {
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
        assert_eq!(a.scheme, Scheme::Local);
        assert_eq!(a.path, "/Users/x/file.txt");
        assert_eq!(a.connection_key(), None);
    }

    #[test]
    fn relative_path_is_local() {
        let a = parse_address("some/rel path").unwrap();
        assert_eq!(a.scheme, Scheme::Local);
        assert_eq!(a.path, "some/rel path");
    }

    #[test]
    fn file_url_maps_to_local_path() {
        let a = parse_address("file:///Users/x").unwrap();
        assert_eq!(a.scheme, Scheme::Local);
        assert_eq!(a.path, "/Users/x");
    }

    #[test]
    fn sftp_authority_is_parsed() {
        let a = parse_address("sftp://alice@host.example:2222/srv/data").unwrap();
        assert_eq!(a.scheme, Scheme::Sftp);
        assert_eq!(a.user.as_deref(), Some("alice"));
        assert_eq!(a.host.as_deref(), Some("host.example"));
        assert_eq!(a.port, Some(2222));
        assert_eq!(a.path, "/srv/data");
        assert_eq!(
            a.connection_key().as_deref(),
            Some("alice@host.example:2222")
        );
    }

    #[test]
    fn sftp_defaults_user_and_port() {
        let a = parse_address("sftp://host/only").unwrap();
        assert_eq!(a.user, None);
        assert_eq!(a.host.as_deref(), Some("host"));
        assert_eq!(a.port, None);
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
            assert_eq!(a.scheme, scheme, "scheme of {}", raw);
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
        assert_eq!(path, "/tmp/x");
    }
}
