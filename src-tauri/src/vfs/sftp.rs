// ============================================================
// SFTP provider (ticket 022).
//
// Implements the VFS `Provider` trait over SSH/SFTP: the SSH transport
// itself lives in `net::ssh` (shared with the terminal, ticket 026),
// this module opens the `sftp` subsystem on it via russh-sftp. The
// client is async; the trait (and the threaded transfer engine) are
// synchronous, so every call is driven through the shared Tokio runtime
// in `net::session` via `block_on`. The transfer streams
// (`open_read`/`create_write`) return sync adapters around async SFTP
// file handles.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{AppError, AppResult};
use crate::net::hostkey::ConnectError;
use crate::net::session::runtime;
use crate::net::ssh::SshSession;
use russh_sftp::client::SftpSession;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A live SFTP connection: the SFTP session plus the SSH connection it
/// runs on. Owned by `net::session`'s registry.
pub struct SftpConnection {
    sftp: SftpSession,
    // Keeps the SSH connection alive and serves the terminal (026).
    ssh: Arc<SshSession>,
}

/// Opens an SSH connection, authenticates, starts an SFTP session and
/// returns the ready provider. Blocking wrapper around the async client
/// (runs on the shared runtime).
pub fn connect(
    profile: &crate::net::session::Profile,
    secret: Option<&str>,
) -> Result<Arc<dyn Provider>, ConnectError> {
    let conn = runtime().block_on(connect_async(profile, secret))?;
    Ok(SftpProvider::shared(
        profile.connection_key(),
        Arc::new(conn),
    ))
}

async fn connect_async(
    profile: &crate::net::session::Profile,
    secret: Option<&str>,
) -> Result<SftpConnection, ConnectError> {
    let ssh = SshSession::connect(profile, secret).await?;

    let channel = ssh.open_channel().await?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| AppError::from(format!("SFTP-Subsystem fehlgeschlagen: {}", e)))?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| AppError::from(format!("SFTP-Sitzung fehlgeschlagen: {}", e)))?;

    Ok(SftpConnection {
        sftp,
        ssh: Arc::new(ssh),
    })
}

// ---------- Provider ----------

/// VFS provider bound to one live SFTP connection.
pub struct SftpProvider {
    key: String,
    conn: Arc<SftpConnection>,
}

impl SftpProvider {
    /// The provider behind one connection, as the shared trait object
    /// the registry stores. Named `shared` rather than `new` because it
    /// hands back an `Arc<dyn Provider>`, not a `Self`.
    pub fn shared(key: String, conn: Arc<SftpConnection>) -> Arc<dyn Provider> {
        Arc::new(SftpProvider { key, conn })
    }

    fn sftp(&self) -> &SftpSession {
        &self.conn.sftp
    }
}

/// POSIX path string for the SFTP protocol.
fn p(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Child path inside a remote directory, as a POSIX path string.
fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{}{}", dir, name)
    } else {
        format!("{}/{}", dir, name)
    }
}

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

impl Provider for SftpProvider {
    fn store_id(&self) -> String {
        format!("sftp:{}", self.key)
    }

    fn ssh(&self) -> Option<Arc<SshSession>> {
        Some(self.conn.ssh.clone())
    }

    /// The SSH transport carries both the SFTP subsystem and the
    /// terminal, so its handle answers for the whole connection.
    fn transport_alive(&self) -> bool {
        !self.conn.ssh.is_closed()
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        let dir = runtime()
            .block_on(self.sftp().read_dir(path))
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;

        let mut entries = Vec::new();
        for item in dir {
            let name = item.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let attrs = item.metadata();
            let mode = attrs.permissions.unwrap_or(0);
            let is_symlink = attrs.is_symlink();
            // SFTP `READDIR` attributes are link-level, exactly like a local
            // `lstat`: a symlink to a directory would arrive with `is_dir =
            // false` and the target string's length as its size (TICKET-018).
            // `SSH_FXP_STAT` follows links, so resolve — one extra round trip
            // per symlink, none for plain entries, which keeps the common case
            // (directories without links) at a single `READDIR` as before.
            let target = if is_symlink {
                runtime()
                    .block_on(self.sftp().metadata(join(path, &name)))
                    .ok()
            } else {
                None
            };
            // A broken link has no target: fall back to the link's own
            // attributes so it stays listed as a 0-byte file.
            let is_dir = target.as_ref().is_some_and(|t| t.is_dir());
            let size = match &target {
                _ if is_dir => 0,
                Some(t) => t.size.unwrap_or(0),
                None if is_symlink => 0,
                None => attrs.size.unwrap_or(0),
            };
            entries.push(DirEntry {
                hidden: name.starts_with('.'),
                readonly: mode != 0 && mode & 0o200 == 0,
                executable: !is_dir && mode & 0o111 != 0,
                name,
                is_dir,
                is_symlink,
                size,
                modified: attrs.mtime.unwrap_or(0) as u64,
                // Link-level, so the frontend still renders `l` and the
                // symlink colour rule.
                mode: attrs.permissions,
            });
        }

        // Return the full address so the pane keeps its remote context;
        // subsequent navigation composes `sftp://key/...` paths that
        // resolve back to this provider.
        Ok(DirListing {
            path: format!("sftp://{}{}", self.key, path),
            entries,
        })
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let attrs = runtime()
            .block_on(self.sftp().metadata(p(path)))
            .map_err(to_io)?;
        Ok(Stat {
            is_dir: attrs.is_dir(),
            len: attrs.size.unwrap_or(0),
        })
    }

    fn exists(&self, path: &Path) -> bool {
        runtime().block_on(self.sftp().metadata(p(path))).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let base = p(path);
        let dir = runtime()
            .block_on(self.sftp().read_dir(&base))
            .map_err(to_io)?;
        let mut out = Vec::new();
        for item in dir {
            let name = item.file_name();
            if name == "." || name == ".." {
                continue;
            }
            out.push(Path::new(&base).join(name));
        }
        Ok(out)
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        let file = runtime()
            .block_on(self.sftp().open(p(path)))
            .map_err(to_io)?;
        Ok(Box::new(SyncSftpFile { file }))
    }

    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        let file = runtime()
            .block_on(self.sftp().create(p(path)))
            .map_err(to_io)?;
        Ok(Box::new(SyncSftpFile { file }))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        runtime().block_on(async {
            let mut f = self.sftp().create(p(path)).await.map_err(to_io)?;
            f.write_all(data).await.map_err(to_io)?;
            f.shutdown().await.map_err(to_io)?;
            Ok(())
        })
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        // SFTP has no recursive mkdir — create each missing component.
        let full = p(path);
        let mut acc = String::new();
        for part in full.split('/') {
            if part.is_empty() {
                acc.push('/');
                continue;
            }
            if !acc.ends_with('/') {
                acc.push('/');
            }
            acc.push_str(part);
            // Ignore "already exists" — only a real failure at the end matters.
            let _ = runtime().block_on(self.sftp().create_dir(&acc));
        }
        // Confirm the target now exists as a directory.
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
        runtime()
            .block_on(self.sftp().rename(p(from), p(to)))
            .map_err(to_io)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        runtime()
            .block_on(self.sftp().remove_file(p(path)))
            .map_err(to_io)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        runtime().block_on(remove_dir_all_async(self.sftp(), p(path)))
    }

    // ----- Failure-safe transfers (ticket 029) -----

    /// Commit is one `rename` inside the same connection.
    fn supports_staging(&self) -> bool {
        true
    }

    /// SFTP `rename` (v3) refuses an existing target, so a
    /// replacement goes through the backup rename.
    fn rename_replaces(&self) -> bool {
        false
    }
}

/// Recursively removes a remote directory (SFTP `rmdir` requires the
/// directory to be empty first).
fn remove_dir_all_async<'a>(
    sftp: &'a SftpSession,
    path: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let dir = sftp.read_dir(&path).await.map_err(to_io)?;
        for item in dir {
            let name = item.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = format!("{}/{}", path.trim_end_matches('/'), name);
            if item.metadata().is_dir() {
                remove_dir_all_async(sftp, child).await?;
            } else {
                sftp.remove_file(child).await.map_err(to_io)?;
            }
        }
        sftp.remove_dir(path).await.map_err(to_io)
    })
}

// ---------- Sync bridge over an async SFTP file ----------

/// Wraps an async russh-sftp file as a synchronous `Read`/`Write` by
/// driving it on the shared runtime. Used by the transfer engine.
struct SyncSftpFile {
    file: russh_sftp::client::fs::File,
}

impl Read for SyncSftpFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        runtime().block_on(self.file.read(buf))
    }
}

impl Write for SyncSftpFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        runtime().block_on(self.file.write(buf))
    }
    fn flush(&mut self) -> io::Result<()> {
        runtime().block_on(self.file.flush())
    }
}

impl Drop for SyncSftpFile {
    fn drop(&mut self) {
        // Flush buffered writes and close the handle cleanly.
        let _ = runtime().block_on(async {
            let _ = self.file.flush().await;
            self.file.shutdown().await
        });
    }
}
