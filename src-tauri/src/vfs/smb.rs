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

use super::listing::{self, ListingBudget};
use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{code, AppError, AppResult};
use crate::net::session::Profile;
use smb::binrw_util::sized_string::SizedWideString;
use smb::resource::{FileCreateArgs, Resource};
use smb::{
    Client, ClientConfig, CreateOptions, FileAccessMask, FileAttributes,
    FileIdBothDirectoryInformation, FileRenameInformation, UncPath,
};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

/// The port SMB is served on unless a profile says otherwise.
const DEFAULT_PORT: u16 = 445;

/// Opens an SMB connection and returns the ready provider.
pub fn connect(profile: &Profile, secret: Option<&str>) -> AppResult<Arc<dyn Provider>> {
    crate::net::security::check(profile)?;
    // The port belongs in the connection config — `UncPath` carries only
    // the server name, so a profile pointing at a non-standard port used
    // to be silently connected to 445 instead (ticket 038).
    let mut config = ClientConfig::default();
    config.connection.timeout = Some(std::time::Duration::from_secs(u64::from(
        profile.monitor.clone().unwrap_or_default().timeout_seconds,
    )));
    config.connection.encryption_mode = smb::connection::EncryptionMode::Required;
    config.connection.allow_unsigned_guest_access = false;
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

    let mut provider = SmbProvider {
        security: crate::net::security::TransportInfo::connected(profile, true),
        key: profile.connection_key(),
        server: profile.host.clone(),
        client,
        user,
        password,
        connected: AttachGates::default(),
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
                .shares(&mut ListingBudget::default())
                .map_err(|e| AppError::from(format!("Verbindung fehlgeschlagen: {}", e)))?;
        }
    }

    if let Ok(connection) = provider.client.get_connection(&provider.server) {
        if let Some(info) = connection.conn_info() {
            provider.security.protocol = Some(format!("SMB {:?}", info.negotiation.dialect_rev));
            provider.security.cipher = info
                .negotiation
                .encryption_cipher
                .map(|cipher| format!("{cipher:?}"));
        }
    }
    provider.security.signing = Some(true);
    Ok(Arc::new(provider))
}

/// VFS provider bound to one SMB server.
pub struct SmbProvider {
    security: crate::net::security::TransportInfo,
    key: String,
    server: String,
    client: Client,
    user: String,
    password: String,
    /// Shares that have already been attached on this connection.
    connected: AttachGates,
}

/// One attach gate per share, so a share is tree-connected exactly once
/// even when two operations reach it at the same time (ticket 339).
///
/// The map lock is only held while a gate is looked up; the gate itself is
/// held across the connect. Two operations on the *same* share therefore
/// serialize — the second finds the share already attached and returns —
/// while two operations on *different* shares still attach concurrently,
/// which is what a remote→remote copy on one server needs.
#[derive(Default)]
struct AttachGates {
    gates: Mutex<HashMap<String, Arc<Mutex<bool>>>>,
}

impl AttachGates {
    fn gate(&self, share: &str) -> Arc<Mutex<bool>> {
        self.gates
            .lock()
            .unwrap()
            .entry(share.to_string())
            .or_default()
            .clone()
    }

    /// Runs `connect` unless this share is already attached.
    fn attach(&self, share: &str, connect: impl FnOnce() -> AppResult<()>) -> AppResult<()> {
        let gate = self.gate(share);
        let mut attached = gate.lock().unwrap();
        if *attached {
            return Ok(());
        }
        connect()?;
        *attached = true;
        Ok(())
    }

    /// Any share known to be attached, for the health probe. Never waits on
    /// a gate: an in-flight connect is not an answer about an existing one.
    fn any_attached(&self) -> Option<String> {
        self.gates
            .lock()
            .unwrap()
            .iter()
            .find(|(_, gate)| gate.try_lock().is_ok_and(|attached| *attached))
            .map(|(share, _)| share.clone())
    }
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
fn p(path: &crate::vfs::StreamPath) -> String {
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
        self.connected.attach(share, || {
            let unc = UncPath::new(&self.server)
                .and_then(|u| u.with_share(share))
                .map_err(|e| AppError::from(format!("Ungültiger SMB-Pfad: {}", e)))?;
            self.client
                .share_connect(&unc, &self.user, self.password.clone())
                .map_err(|e| AppError::from(format!("Share '{}' nicht erreichbar: {}", share, e)))
        })
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
    fn shares(&self, budget: &mut ListingBudget) -> AppResult<Vec<DirEntry>> {
        // Enumerating shares talks to the `srvsvc` named pipe, which lives
        // on the hidden IPC$ share — that has to be attached first.
        self.connected.attach(IPC_SHARE, || {
            self.client
                .ipc_connect(&self.server, &self.user, self.password.clone())
                .map_err(|e| AppError::from(format!("IPC$ nicht erreichbar: {}", e)))
        })?;
        // Not every server accepts the RPC transfer syntax the client
        // proposes (Samba, for instance, rejects the bind). This needs its
        // own code, not the generic internal error: the guidance is the
        // whole point of the message, and `.detail` is never displayed
        // (ticket 341). The raw protocol error stays in the diagnostics.
        let shares = self.client.list_shares(&self.server).map_err(|e| {
            AppError::new(code::NET_SHARE_LISTING_UNSUPPORTED)
                .provider("smb")
                .op("list")
                .action(crate::errors::ErrorAction::Unsupported)
                .detail(e)
        })?;
        let mut entries = Vec::new();
        for share in &shares {
            let name = share
                .netname
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            budget.charge(&name)?;
            // Administrative shares end with `$` — keep them, but hidden.
            entries.push(DirEntry {
                hidden: name.ends_with('$'),
                readonly: false,
                executable: false,
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: 0,
                mode: None,
                name,
            });
        }
        Ok(entries)
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
    fn dir_entries(&self, path: &str, budget: &mut ListingBudget) -> AppResult<Vec<DirEntry>> {
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
                // Charged before the entry is built, so an oversized
                // enumeration ends here instead of being read to its end.
                budget.charge(&name)?;
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

    /// Deletes a reparse point itself rather than what it points at.
    ///
    /// `FILE_OPEN_REPARSE_POINT` keeps the server from resolving the link,
    /// so the junction disappears and its target stays untouched.
    fn remove_reparse_point(&self, path: &str, is_dir: bool) -> io::Result<()> {
        let mut args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_delete(true));
        args.options = CreateOptions::new()
            .with_delete_on_close(true)
            .with_open_reparse_point(true)
            .with_directory_file(is_dir);
        close_resource(self.open(path, &args).map_err(to_io)?)
    }
}

impl Provider for SmbProvider {
    fn health_check(&self, _timeout: std::time::Duration) -> io::Result<super::Health> {
        // The connection timeout was captured from this profile at connect.
        // Use an already attached share: a health probe must not create a login.
        let share = self.connected.any_attached();
        let Some(share) = share else {
            return Ok(super::Health::Unsupported);
        };
        let unc = UncPath::new(&self.server)
            .and_then(|u| u.with_share(&share))
            .map_err(to_io)?;
        let session = self.client.get_session(&unc).map_err(to_io)?;
        session.echo().map_err(to_io)?;
        Ok(super::Health::Alive)
    }

    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        Some(self.security.clone())
    }
    fn store_id(&self) -> String {
        format!("smb:{}", self.key)
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        super::provider_path::validate(&crate::vfs::StreamPath::remote(path)?, super::Scheme::Smb)?;
        // The connection root lists the server's shares.
        let mut budget = ListingBudget::default();
        let entries = match split_share(path) {
            None => self.shares(&mut budget)?,
            Some(_) => self.dir_entries(path, &mut budget)?,
        };
        // Full address so the pane keeps its remote context.
        Ok(DirListing {
            path: super::location::remote_uri("smb", &self.key, path)?,
            entries,
        })
    }

    fn stat(&self, path: &crate::vfs::StreamPath) -> io::Result<Stat> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
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
        let entries = self
            .dir_entries(parent, &mut ListingBudget::default())
            .map_err(to_io)?;
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

    fn exists(&self, path: &crate::vfs::StreamPath) -> bool {
        if super::provider_path::validate(path, super::Scheme::Smb).is_err() {
            return false;
        }
        self.stat(path).is_ok()
    }

    fn is_dir(&self, path: &crate::vfs::StreamPath) -> bool {
        if super::provider_path::validate(path, super::Scheme::Smb).is_err() {
            return false;
        }
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &crate::vfs::StreamPath) -> io::Result<Vec<crate::vfs::StreamPath>> {
        self.read_dir_bounded(path, listing::MAX_ENTRIES)
    }

    fn read_dir_bounded(
        &self,
        path: &crate::vfs::StreamPath,
        limit: usize,
    ) -> io::Result<Vec<crate::vfs::StreamPath>> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        let base = p(path);
        let mut budget = ListingBudget::new(limit);
        let entries = match split_share(&base) {
            None => self.shares(&mut budget).map_err(to_io)?,
            Some(_) => self.dir_entries(&base, &mut budget).map_err(to_io)?,
        };
        entries
            .iter()
            .map(|e| path.join(&e.name))
            .collect::<io::Result<Vec<_>>>()
    }

    fn open_read(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Read + Send>> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        let resource = self.open_existing(&p(path)).map_err(to_io)?;
        let file = resource.unwrap_file();
        Ok(Box::new(SmbReader {
            file: Some(file),
            offset: 0,
        }))
    }

    fn create_write(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Write + Send>> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        let args = FileCreateArgs::make_overwrite(FileAttributes::new(), CreateOptions::new());
        let resource = self.open(&p(path), &args).map_err(to_io)?;
        let file = resource.unwrap_file();
        Ok(Box::new(SmbWriter {
            file: Some(file),
            offset: 0,
        }))
    }

    fn write(&self, path: &crate::vfs::StreamPath, data: &[u8]) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        let mut w = self.create_write(path)?;
        w.write_all(data)?;
        w.flush()
    }

    fn create_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
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
            if self.is_dir(&crate::vfs::StreamPath::remote(&acc).map_err(io::Error::other)?) {
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

    fn rename(&self, from: &crate::vfs::StreamPath, to: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(from, super::Scheme::Smb)?;
        super::provider_path::validate(to, super::Scheme::Smb)?;
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

    fn remove_file(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        // DELETE_ON_CLOSE is the SMB way to unlink.
        let mut args = FileCreateArgs::make_open_existing(FileAccessMask::new().with_delete(true));
        args.options = CreateOptions::new().with_delete_on_close(true);
        close_resource(self.open(&p(path), &args).map_err(to_io)?)
    }

    fn remove_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Smb)?;
        let base = p(path);
        if split_share(&base).is_none() {
            return Err(io::Error::other(
                "Der Verbindungsroot listet Shares und kann nicht gelöscht werden",
            ));
        }
        // A directory must be empty before it can be deleted. The listing's
        // own attributes decide how each child goes: `stat` resolves a
        // reparse point, so asking it whether the child is a directory would
        // send a junction pointing back at an ancestor into unbounded
        // recursion (ticket 319).
        let mut budget = ListingBudget::default();
        for entry in self.dir_entries(&base, &mut budget).map_err(to_io)? {
            let child = path.join(&entry.name)?;
            match child_action(&entry) {
                ChildAction::Descend => self.remove_dir_all(&child)?,
                ChildAction::DeleteLink { is_dir } => {
                    self.remove_reparse_point(&p(&child), is_dir)?
                }
                ChildAction::DeleteFile => self.remove_file(&child)?,
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

/// How `remove_dir_all` has to treat one listed child.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChildAction {
    /// A plain directory: recurse into it.
    Descend,
    /// A reparse point (junction/symlink): delete the link, never its
    /// target — following it can loop back into an ancestor (ticket 319).
    DeleteLink { is_dir: bool },
    /// A plain file: unlink it.
    DeleteFile,
}

/// Classifies a listed child from its link-level directory attributes.
fn child_action(entry: &DirEntry) -> ChildAction {
    match (entry.is_dir, entry.is_symlink) {
        (is_dir, true) => ChildAction::DeleteLink { is_dir },
        (true, false) => ChildAction::Descend,
        (false, false) => ChildAction::DeleteFile,
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
    use super::{child_action, split_share, AttachGates, ChildAction};
    use crate::commands::fs::dir::DirEntry;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_callers_attach_a_share_exactly_once() {
        let gates = Arc::new(AttachGates::default());
        let connects = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(8));
        let mut threads = Vec::new();
        for _ in 0..8 {
            let (gates, connects, start) = (gates.clone(), connects.clone(), start.clone());
            threads.push(std::thread::spawn(move || {
                start.wait();
                gates.attach("data", || {
                    connects.fetch_add(1, Ordering::SeqCst);
                    // Widen the window the old check-then-act version left open.
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    Ok(())
                })
            }));
        }
        for t in threads {
            t.join().unwrap().expect("no caller sees a spurious error");
        }
        assert_eq!(connects.load(Ordering::SeqCst), 1);
        assert_eq!(gates.any_attached().as_deref(), Some("data"));
    }

    #[test]
    fn a_failed_attach_is_retried_and_never_marks_the_share_attached() {
        let gates = AttachGates::default();
        assert!(gates
            .attach("data", || Err("offline".to_string().into()))
            .is_err());
        assert_eq!(gates.any_attached(), None);
        assert!(gates.attach("data", || Ok(())).is_ok());
        assert_eq!(gates.any_attached().as_deref(), Some("data"));
    }

    fn entry(is_dir: bool, is_symlink: bool) -> DirEntry {
        DirEntry {
            name: "child".into(),
            hidden: false,
            readonly: false,
            executable: false,
            is_dir,
            is_symlink,
            size: 0,
            modified: 0,
            mode: None,
        }
    }

    #[test]
    fn a_directory_reparse_point_is_never_descended_into() {
        assert_eq!(
            child_action(&entry(true, true)),
            ChildAction::DeleteLink { is_dir: true }
        );
        assert_eq!(
            child_action(&entry(false, true)),
            ChildAction::DeleteLink { is_dir: false }
        );
    }

    #[test]
    fn plain_entries_keep_their_ordinary_handling() {
        assert_eq!(child_action(&entry(true, false)), ChildAction::Descend);
        assert_eq!(child_action(&entry(false, false)), ChildAction::DeleteFile);
    }

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
