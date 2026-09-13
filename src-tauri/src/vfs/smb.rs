// ============================================================
// SMB/CIFS provider (ticket 023).
//
// Implements the VFS `Provider` trait over SMB2/3 using the pure-Rust
// `smb` crate in its `multi_threaded` (synchronous) mode — no
// libsmbclient/Samba C dependency, so the provider also works on
// Windows.
//
// Path model: an `smb://user@host:445/...` address maps to
// `\\host\<share>\<rest>`, i.e. the **first path segment is the share**.
// Listing the connection root therefore enumerates the server's shares
// (server → shares → directories, per the ticket); everything below
// operates inside a share. Shares are connected lazily on first use and
// remembered for the lifetime of the connection.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{AppError, AppResult};
use crate::net::session::Profile;
use smb::binrw_util::sized_string::SizedWideString;
use smb::resource::{FileCreateArgs, Resource};
use smb::{
    Client, ClientConfig, CreateOptions, FileAccessMask, FileAttributes,
    FileIdBothDirectoryInformation, FileRenameInformation, UncPath,
};
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The port SMB is served on unless a profile says otherwise.
const DEFAULT_PORT: u16 = 445;

/// Opens an SMB connection and returns the ready provider.
pub fn connect(profile: &Profile, secret: Option<&str>) -> AppResult<Arc<dyn Provider>> {
    // The port belongs in the connection config — `UncPath` carries only
    // the server name, so a profile pointing at a non-standard port used
    // to be silently connected to 445 instead (ticket 038).
    let mut config = ClientConfig::default();
    if profile.port != 0 && profile.port != DEFAULT_PORT {
        config.connection.port = Some(profile.port);
    }
    let client = Client::new(config);

    // Domain/workgroup accounts are written `DOMAIN\user` (or `user@domain`),
    // which the SMB layer understands directly.
    let user = profile.user.clone();
    let password = if profile.auth == "anonymous" {
        String::new()
    } else {
        secret.unwrap_or_default().to_string()
    };

    let provider = SmbProvider {
        key: profile.connection_key(),
        server: profile.host.clone(),
        client,
        user,
        password,
        connected: Mutex::new(HashSet::new()),
    };

    // Probe: connecting the start path's share (or listing the shares)
    // surfaces auth/host errors here instead of on first navigation.
    let start = if profile.start_path.is_empty() {
        "/"
    } else {
        &profile.start_path
    };
    match split_share(start) {
        Some((share, _)) => provider.ensure_share(&share)?,
        None => {
            provider
                .shares()
                .map_err(|e| AppError::from(format!("Verbindung fehlgeschlagen: {}", e)))?;
        }
    }

    Ok(Arc::new(provider))
}

/// VFS provider bound to one SMB server.
pub struct SmbProvider {
    key: String,
    server: String,
    client: Client,
    user: String,
    password: String,
    /// Shares that have already been attached on this connection.
    connected: Mutex<HashSet<String>>,
}

/// Marker for the hidden IPC$ share in the `connected` set (it is
/// attached separately from the normal file shares).
const IPC_SHARE: &str = "IPC$";

/// Splits a provider path into `(share, rest)`.
/// `/data/dir/file` → `("data", "dir/file")`; `/` → `None`.
fn split_share(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.split_once('/') {
        Some((share, rest)) => Some((share.to_string(), rest.trim_matches('/').to_string())),
        None => Some((trimmed.to_string(), String::new())),
    }
}

/// Normalizes a `Path` to the provider's POSIX-ish form.
fn p(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        "/".to_string()
    } else {
        s
    }
}

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

/// Windows FILETIME → Unix seconds.
fn unix_secs(t: impl Into<std::time::SystemTime>) -> u64 {
    t.into()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl SmbProvider {
    /// Attaches a share once per connection.
    fn ensure_share(&self, share: &str) -> AppResult<()> {
        {
            let done = self.connected.lock().unwrap();
            if done.contains(share) {
                return Ok(());
            }
        }
        let unc = UncPath::new(&self.server)
            .and_then(|u| u.with_share(share))
            .map_err(|e| AppError::from(format!("Ungültiger SMB-Pfad: {}", e)))?;
        self.client
            .share_connect(&unc, &self.user, self.password.clone())
            .map_err(|e| AppError::from(format!("Share '{}' nicht erreichbar: {}", share, e)))?;
        self.connected.lock().unwrap().insert(share.to_string());
        Ok(())
    }

    /// UNC path for a provider path (share must be attached first).
    fn unc_of(&self, path: &str) -> AppResult<UncPath> {
        let (share, rest) =
            split_share(path).ok_or_else(|| "Pfad enthält keinen Share".to_string())?;
        self.ensure_share(&share)?;
        let mut unc = UncPath::new(&self.server)
            .and_then(|u| u.with_share(&share))
            .map_err(|e| AppError::from(format!("Ungültiger SMB-Pfad: {}", e)))?;
        if !rest.is_empty() {
            // SMB uses backslashes inside a share.
            unc = unc.with_path(&rest.replace('/', "\\"));
        }
        Ok(unc)
    }

    /// The server's shares, as directory entries.
    fn shares(&self) -> AppResult<Vec<DirEntry>> {
        // Enumerating shares talks to the `srvsvc` named pipe, which lives
        // on the hidden IPC$ share — that has to be attached first.
        {
            let mut done = self.connected.lock().unwrap();
            if !done.contains(IPC_SHARE) {
                self.client
                    .ipc_connect(&self.server, &self.user, self.password.clone())
                    .map_err(|e| AppError::from(format!("IPC$ nicht erreichbar: {}", e)))?;
                done.insert(IPC_SHARE.to_string());
            }
        }
        // Not every server accepts the RPC transfer syntax the client
        // proposes (Samba, for instance, rejects the bind) — say what to
        // do instead of surfacing a raw protocol error.
        let shares = self.client.list_shares(&self.server).map_err(|e| {
            format!(
                "Shares konnten nicht aufgelistet werden ({}). \
                 Dieser Server unterstützt die Share-Auflistung nicht — \
                 bitte im Verbindungsprofil einen Startpfad mit Share angeben, \
                 z. B. /Freigabe.",
                e
            )
        })?;
        Ok(shares
            .iter()
            .map(|s| {
                s.netname
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default()
            })
            .filter(|name| !name.is_empty())
            // Administrative shares end with `$` — keep them, but hidden.
            .map(|name| DirEntry {
                hidden: name.ends_with('$'),
                readonly: false,
                executable: false,
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: 0,
                mode: None,
                name,
            })
            .collect())
    }

    /// Opens a resource with the given create arguments.
    fn open(&self, path: &str, args: &FileCreateArgs) -> AppResult<Resource> {
        let unc = self.unc_of(path)?;
        self.client
            .create_file(&unc, args)
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))
    }

    /// Read-only open of an existing entry.
    fn open_existing(&self, path: &str) -> AppResult<Resource> {
        self.open(
            path,
            &FileCreateArgs::make_open_existing(FileAccessMask::new().with_generic_read(true)),
        )
    }

    /// Directory entries inside a share.
    ///
    /// The directory handle is closed before returning — leaving it open
    /// locks the directory server-side and makes a later rename/delete
    /// fail with a sharing violation.
    fn dir_entries(&self, path: &str) -> AppResult<Vec<DirEntry>> {
        let resource = self.open_existing(path)?;
        let dir = resource
            .is_dir()
            .then(|| resource.unwrap_dir())
            .ok_or_else(|| AppError::from(format!("{}: kein Verzeichnis", path)))?;

        // Scoped so the borrowing iterator is dropped before the close.
        let collected = {
            let mut entries = Vec::new();
            let iter = dir
                .query::<FileIdBothDirectoryInformation>("*")
                .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
            for item in iter {
                let info = item.map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
                let name = info.file_name.to_string();
                if name == "." || name == ".." {
                    continue;
                }
                let is_dir = info.file_attributes.directory();
                entries.push(DirEntry {
                    hidden: info.file_attributes.hidden() || name.starts_with('.'),
                    readonly: info.file_attributes.readonly(),
                    executable: false,
                    name,
                    is_dir,
                    is_symlink: info.file_attributes.reparse_point(),
                    size: if is_dir { 0 } else { info.end_of_file },
                    modified: unix_secs(info.last_write_time),
                    mode: None,
                });
            }
            entries
        };
        let _ = dir.close();
        Ok(collected)
    }
}

impl Provider for SmbProvider {
    fn store_id(&self) -> String {
        format!("smb:{}", self.key)
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        // The connection root lists the server's shares.
        let entries = match split_share(path) {
            None => self.shares()?,
            Some(_) => self.dir_entries(path)?,
        };
        // Full address so the pane keeps its remote context.
        Ok(DirListing {
            path: format!("smb://{}{}", self.key, path),
            entries,
        })
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let full = p(path);
        // Root and a bare share are always directories.
        let Some((_, rest)) = split_share(&full) else {
            return Ok(Stat {
                is_dir: true,
                len: 0,
            });
        };
        if rest.is_empty() {
            return Ok(Stat {
                is_dir: true,
                len: 0,
            });
        }
        // Look the entry up in its parent listing (one round trip, and
        // it yields size + kind together).
        let (parent, name) = full.rsplit_once('/').unwrap_or(("/", full.as_str()));
        let parent = if parent.is_empty() { "/" } else { parent };
        let entries = self.dir_entries(parent).map_err(to_io)?;
        entries
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| Stat {
                is_dir: e.is_dir,
                len: e.size,
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{}: nicht gefunden", full))
            })
    }

    fn exists(&self, path: &Path) -> bool {
        self.stat(path).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let base = p(path);
        let entries = match split_share(&base) {
            None => self.shares().map_err(to_io)?,
            Some(_) => self.dir_entries(&base).map_err(to_io)?,
        };
        Ok(entries
            .iter()
            .map(|e| Path::new(&base).join(&e.name))
            .collect())
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        let resource = self.open_existing(&p(path)).map_err(to_io)?;
        let file = resource.unwrap_file();
        Ok(Box::new(SmbReader {
            file: Some(file),
            offset: 0,
        }))
    }

    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        let args = FileCreateArgs::make_overwrite(FileAttributes::new(), CreateOptions::new());
        let resource = self.open(&p(path), &args).map_err(to_io)?;
        let file = resource.unwrap_file();
        Ok(Box::new(SmbWriter {
            file: Some(file),
            offset: 0,
        }))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let mut w = self.create_write(path)?;
        w.write_all(data)?;
        w.flush()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        let full = p(path);
        let Some((share, rest)) = split_share(&full) else {
            return Ok(()); // Wurzel existiert immer
        };
        self.ensure_share(&share).map_err(to_io)?;
        if rest.is_empty() {
            return Ok(()); // Shares werden nicht angelegt
        }

        // SMB creates one directory at a time — walk the components.
        let mut acc = format!("/{}", share);
        for part in rest.split('/') {
            if part.is_empty() {
                continue;
            }
            acc.push('/');
            acc.push_str(part);
            if self.is_dir(Path::new(&acc)) {
                continue;
            }
            let args = FileCreateArgs::make_create_new(
                FileAttributes::new().with_directory(true),
                CreateOptions::new().with_directory_file(true),
            );
            // Ignore "already exists"; the check below reports real failures.
            // The new handle must be closed, or the directory stays locked.
            if let Ok(resource) = self.open(&acc, &args) {
                let _ = close_resource(resource);
            }
        }
        if self.is_dir(path) {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "Ordner konnte nicht angelegt werden: {}",
                full
            )))
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let (_, to_rest) =
            split_share(&p(to)).ok_or_else(|| io::Error::other("Zielpfad enthält keinen Share"))?;
        // A rename needs DELETE on the source; attribute rights are added
        // because some servers reject SET_INFO on a delete-only handle.
        let resource = self
            .open(
                &p(from),
                &FileCreateArgs::make_open_existing(
                    FileAccessMask::new()
                        .with_delete(true)
                        .with_file_read_attributes(true)
                        .with_file_write_attributes(true),
                ),
            )
            .map_err(to_io)?;
        let info = FileRenameInformation {
            replace_if_exists: false.into(),
            root_directory: 0,
            file_name: SizedWideString::from(to_rest.replace('/', "\\").as_str()),
        };
        match resource {
            Resource::File(f) => {
                f.set_info(info).map_err(to_io)?;
                f.close().map_err(to_io)
            }
            Resource::Directory(d) => {
                d.set_info(info).map_err(to_io)?;
                d.close().map_err(to_io)
            }
            _ => Ok(()),
        }
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        // DELETE_ON_CLOSE is the SMB way to unlink.
        let mut args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_delete(true));
        args.options = CreateOptions::new().with_delete_on_close(true);
        close_resource(self.open(&p(path), &args).map_err(to_io)?)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        // A directory must be empty before it can be deleted.
        for child in self.read_dir(path)? {
            if self.is_dir(&child) {
                self.remove_dir_all(&child)?;
            } else {
                self.remove_file(&child)?;
            }
        }
        let mut args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_delete(true));
        args.options = CreateOptions::new()
            .with_delete_on_close(true)
            .with_directory_file(true);
        close_resource(self.open(&p(path), &args).map_err(to_io)?)
    }

    // ----- Failure-safe transfers (ticket 029) -----

    /// Commit is one `rename` inside the same connection.
    fn supports_staging(&self) -> bool {
        true
    }

    /// The rename here opens the source for delete and issues
    /// SET_INFO; replacing an existing name is not requested, so a
    /// replacement goes through the backup rename.
    fn rename_replaces(&self) -> bool {
        false
    }
}

/// Closes a resource regardless of its kind (the close triggers the
/// pending DELETE_ON_CLOSE, so its error matters).
fn close_resource(resource: Resource) -> io::Result<()> {
    match resource {
        Resource::File(f) => f.close().map_err(to_io),
        Resource::Directory(d) => d.close().map_err(to_io),
        _ => Ok(()),
    }
}

// ---------- Transfer streams ----------
//
// Both stream types own an SMB file handle and **must close it**: an
// open handle keeps the file locked server-side, so a later rename or
// delete fails with a sharing violation.

/// Sequential reader over a positional SMB file handle.
struct SmbReader {
    file: Option<smb::resource::File>,
    offset: u64,
}

impl Read for SmbReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(file) = self.file.as_ref() else {
            return Ok(0);
        };
        let n = file
            .read_block(buf, self.offset, None, false)
            .map_err(to_io)?;
        self.offset += n as u64;
        Ok(n)
    }
}

impl Drop for SmbReader {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = file.close();
        }
    }
}

/// Sequential writer over a positional SMB file handle.
struct SmbWriter {
    file: Option<smb::resource::File>,
    offset: u64,
}

impl SmbWriter {
    /// Flushes and closes the handle. `flush()` calls this so a failed
    /// close reaches the transfer engine instead of being lost in `Drop`.
    fn finish(&mut self) -> io::Result<()> {
        let Some(file) = self.file.take() else {
            return Ok(()); // bereits abgeschlossen
        };
        let flushed = file.flush();
        let closed = file.close().map_err(to_io);
        flushed.and(closed)
    }
}

impl Write for SmbWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(file) = self.file.as_ref() else {
            return Err(io::Error::other("Datei bereits geschlossen"));
        };
        let n = file.write_block(buf, self.offset, None)?;
        self.offset += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.finish()
    }
}

impl Drop for SmbWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::split_share;

    #[test]
    fn root_has_no_share() {
        assert!(split_share("/").is_none());
        assert!(split_share("").is_none());
    }

    #[test]
    fn first_segment_is_the_share() {
        assert_eq!(split_share("/data"), Some(("data".into(), "".into())));
        assert_eq!(
            split_share("/data/sub/file.txt"),
            Some(("data".into(), "sub/file.txt".into()))
        );
    }
}
