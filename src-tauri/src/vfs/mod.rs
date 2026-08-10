// ============================================================
// Virtual file system (VFS) abstraction.
//
// A `Provider` trait hides all file-system operations behind a
// uniform interface. A scheme registry maps an address scheme
// (`local`, later `sftp`/`smb`/`webdav`/`ftp`) to its provider, so
// the commands in `commands/fs/*` operate schema-agnostically —
// the basis for the "identical UI" across local and remote
// locations (Concept.md §10 P4, ticket series 020–026).
//
// This foundation only wires up the `local` provider; the reserved
// remote schemes are recognized by the parser but resolve to a
// clear "not yet supported" error.
// ============================================================

pub mod local;

use crate::commands::fs::dir::DirListing;
use local::LocalProvider;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Address scheme. `Local` is the only implemented provider today;
/// the remaining variants are reserved for the upcoming network
/// providers and are parsed but not yet resolvable.
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
}

/// A parsed address: a scheme plus the provider-local path. Plain
/// local paths (no `scheme://`) map to `Scheme::Local` with the path
/// unchanged, so existing local paths keep working verbatim.
#[derive(Clone, Debug)]
pub struct Address {
    pub scheme: Scheme,
    /// Path as seen by the provider. For `local` this is the raw
    /// file-system path; for remote schemes it is the path portion
    /// after `scheme://[user@host]`.
    pub path: String,
}

/// Parses a raw address string into a scheme + provider-local path.
///
/// - No `scheme://` prefix → a local path, used verbatim.
/// - `file://…` / `local://…` → local path (the leading slashes of an
///   absolute `file:///…` URL stay part of the path).
/// - A reserved remote scheme (`sftp`, `smb`, `webdav`, `ftp`) is
///   accepted here; resolving it fails later with a clear message.
/// - Any other scheme → an error.
pub fn parse_address(raw: &str) -> Result<Address, String> {
    let Some(idx) = raw.find("://") else {
        // No scheme: a plain local path.
        return Ok(Address {
            scheme: Scheme::Local,
            path: raw.to_string(),
        });
    };

    let scheme_str = &raw[..idx];
    let rest = &raw[idx + 3..];
    match Scheme::from_scheme_str(scheme_str) {
        Some(Scheme::Local) => Ok(Address {
            scheme: Scheme::Local,
            // `file:///Users/x` → `/Users/x`; `file://relative` → `relative`.
            path: rest.to_string(),
        }),
        Some(scheme) => Ok(Address {
            scheme,
            // Drop an optional `user@host` authority; keep the path.
            path: remote_path(rest),
        }),
        None => Err(format!("Unbekanntes Adressschema: {}://", scheme_str)),
    }
}

/// Extracts the path portion of a remote address body `user@host/path`.
/// Returns `/` when only an authority is given.
fn remote_path(rest: &str) -> String {
    match rest.find('/') {
        Some(pos) => rest[pos..].to_string(),
        None => "/".to_string(),
    }
}

/// Returns the provider registered for a scheme, or `None` for a
/// reserved-but-unimplemented remote scheme.
pub fn provider_for(scheme: Scheme) -> Option<&'static dyn Provider> {
    static LOCAL: LocalProvider = LocalProvider;
    match scheme {
        Scheme::Local => Some(&LOCAL),
        // Reserved for tickets 022–025; no backend yet.
        Scheme::Sftp | Scheme::Smb | Scheme::Webdav | Scheme::Ftp => None,
    }
}

/// Resolves a raw address to its provider and provider-local path.
/// Unknown schemes and reserved-but-unimplemented schemes return a
/// clear error instead of silently falling back to local.
pub fn resolve(raw: &str) -> Result<(&'static dyn Provider, String), String> {
    let addr = parse_address(raw)?;
    match provider_for(addr.scheme) {
        Some(provider) => Ok((provider, addr.path)),
        None => Err(format!(
            "Schema '{}://' wird noch nicht unterstützt",
            addr.scheme.as_str()
        )),
    }
}

/// Lightweight metadata subset used by the transfer engine.
pub struct Stat {
    pub is_dir: bool,
    pub len: u64,
}

/// Uniform file-system interface. Every provider (local today,
/// remote later) implements these operations; the commands call
/// only through [`resolve`], never `std::fs` directly.
///
/// The nine core operations from the ticket map as follows:
/// `list`, `stat`, `read` = [`open_read`], `write` = [`write`] /
/// [`create_write`], `mkdir` = [`create_dir_all`], `rename`,
/// `delete` = [`remove`]. **copy** and **move** are not single calls
/// but are built from the streaming primitives `open_read` /
/// `create_write` plus `rename` / `remove` (see the transfer engine
/// in `commands/fs/file.rs`), so progress, pause, cancel and
/// checksums work uniformly across providers.
pub trait Provider: Send + Sync {
    /// Which scheme this provider serves.
    fn scheme(&self) -> Scheme;

    /// Full directory listing (drives the `list_dir` command).
    fn list(&self, path: &str) -> Result<DirListing, String>;

    /// Cheap metadata (following symlinks) for the transfer totals.
    fn stat(&self, path: &Path) -> io::Result<Stat>;

    /// Whether the path exists.
    fn exists(&self, path: &Path) -> bool;

    /// Whether the path is a directory (following symlinks).
    fn is_dir(&self, path: &Path) -> bool;

    /// Child paths of a directory (errors propagate).
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;

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
    fn sftp_scheme_parses_but_does_not_resolve() {
        let a = parse_address("sftp://user@host/srv/data").unwrap();
        assert_eq!(a.scheme, Scheme::Sftp);
        assert_eq!(a.path, "/srv/data");
        // Reserved scheme: recognized, but no provider yet.
        assert!(resolve("sftp://user@host/srv/data").is_err());
    }

    #[test]
    fn unknown_scheme_is_error() {
        assert!(parse_address("gopher://host/x").is_err());
        assert!(resolve("gopher://host/x").is_err());
    }

    #[test]
    fn local_address_resolves_to_local_provider() {
        let (p, path) = resolve("/tmp/x").unwrap();
        assert_eq!(p.scheme(), Scheme::Local);
        assert_eq!(path, "/tmp/x");
    }
}
