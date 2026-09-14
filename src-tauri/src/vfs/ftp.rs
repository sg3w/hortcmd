// ============================================================
// FTP / FTPS provider (ticket 025).
//
// Implements the VFS `Provider` trait over FTP using suppaftp's
// **synchronous** client, so no async bridge is needed.
//
// Connection model: FTP is stateful and a control connection can carry
// only one data transfer at a time. A shared, mutex-guarded control
// connection therefore serves the short metadata operations (list,
// stat, mkdir, rename, delete), while `open_read`/`create_write` each
// open their **own** dedicated connection. That keeps a remote→remote
// copy on the same server (read + write at once) from deadlocking on
// the shared connection, at the cost of one login per transferred file.
//
// Passive mode is suppaftp's default (ticket requirement); FTPS is
// supported both explicitly (AUTH TLS after connect) and implicitly,
// with certificate validation by the platform verifier through Rustls.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{code, AppError, AppResult};
use crate::net::session::Profile;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use suppaftp::list::File as FtpFile;
use suppaftp::types::FileType;
use suppaftp::{RustlsConnector, RustlsFtpStream, Status};

/// How TLS is applied to a connection.
#[derive(Clone, Copy, PartialEq)]
enum Tls {
    /// Plain FTP — credentials and data travel unencrypted.
    None,
    /// FTPS, `AUTH TLS` after connecting on the normal port.
    Explicit,
    /// FTPS, TLS from the first byte (classically port 990).
    Implicit,
}

/// Everything needed to open a further connection to the same server.
struct Params {
    profile: Profile,
    fingerprint: Mutex<Option<String>>,
    host: String,
    port: u16,
    user: String,
    password: String,
    tls: Tls,
}

impl Params {
    /// Opens and logs in a fresh connection (passive mode by default).
    fn open(&self) -> AppResult<RustlsFtpStream> {
        crate::net::security::check(&self.profile)?;
        let host = self.host.trim_start_matches('[').trim_end_matches(']');
        let addr = if host.contains(':') {
            format!("[{host}]:{}", self.port)
        } else {
            format!("{host}:{}", self.port)
        };
        let tls = if self.tls == Tls::None {
            None
        } else {
            Some(crate::net::tls_config::config(&self.profile)?)
        };
        let connector = || -> AppResult<RustlsConnector> {
            let (config, _) = tls.as_ref().ok_or_else(|| AppError::new(code::NET_TLS))?;
            Ok(RustlsConnector::from(Arc::new(config.clone())))
        };

        let mut ftp = match self.tls {
            Tls::Implicit => {
                let mut s =
                    RustlsFtpStream::connect_secure_implicit(&addr, connector()?, &self.host)
                        .map_err(|e| {
                            tls.as_ref()
                                .map(|(_, evidence)| evidence.error(&e))
                                .unwrap_or_else(|| {
                                    AppError::from(format!(
                                        "Verbindung zu {} fehlgeschlagen: {}",
                                        addr, e
                                    ))
                                })
                        })?;
                // `AUTH TLS` (explicit) ends with PBSZ 0 + PROT P inside
                // suppaftp; the implicit path does not send them at all,
                // so the control channel is encrypted and every data
                // channel is negotiated in the clear. A server that
                // insists on encrypted data answers "522 Data
                // connections must be encrypted" and nothing transfers;
                // one that does not insist would have carried the file
                // unprotected, which is worse (found by the protocol
                // harness, ticket 038).
                for (command, what) in [("PBSZ 0", "PBSZ"), ("PROT P", "PROT")] {
                    s.custom_command(command, &[Status::CommandOk])
                        .map_err(|e| {
                            AppError::new(code::NET_TLS)
                                .op("ftps-implicit")
                                .detail(format!("{} abgelehnt: {}", what, e))
                        })?;
                }
                s
            }
            _ => {
                let s = RustlsFtpStream::connect(&addr).map_err(|e| {
                    tls.as_ref()
                        .map(|(_, evidence)| evidence.error(&e))
                        .unwrap_or_else(|| {
                            AppError::from(format!("Verbindung zu {} fehlgeschlagen: {}", addr, e))
                        })
                })?;
                if self.tls == Tls::Explicit {
                    s.into_secure(connector()?, &self.host).map_err(|e| {
                        tls.as_ref()
                            .map(|(_, evidence)| evidence.error(&e))
                            .unwrap_or_else(|| AppError::new(code::NET_TLS_PROTOCOL))
                    })?
                } else {
                    s
                }
            }
        };

        ftp.login(&self.user, &self.password).map_err(|e| {
            tls.as_ref()
                .and_then(|(_, evidence)| evidence.failure.lock().ok().and_then(|f| f.clone()))
                .unwrap_or_else(|| AppError::from(format!("Anmeldung fehlgeschlagen: {}", e)))
        })?;

        if let Some((_, evidence)) = &tls {
            if let (Ok(mut fingerprint), Ok(actual)) =
                (self.fingerprint.lock(), evidence.fingerprint.lock())
            {
                *fingerprint = actual.clone();
            }
        }

        // Binary, always. FTP's default transfer type is ASCII (RFC 959),
        // and a server that honours it rewrites every 0x0A on the way
        // past: a 3 MiB file arrives 12 KiB longer and every archive,
        // image and executable is quietly corrupted. vsftpd ignores
        // ASCII mode by default and ProFTPD does not, which is the whole
        // reason this went unnoticed until the protocol harness ran
        // against both (ticket 038).
        ftp.transfer_type(FileType::Binary)
            .map_err(|e| AppError::from(format!("TYPE I abgelehnt: {}", e)))?;
        Ok(ftp)
    }
}

/// Opens an FTP/FTPS connection and returns the ready provider.
pub fn connect(profile: &Profile, secret: Option<&str>) -> AppResult<Arc<dyn Provider>> {
    crate::net::security::check(profile)?;
    // Anonymous FTP is the conventional fallback when no user is given.
    let (user, password) = if profile.auth == "anonymous" || profile.user.is_empty() {
        ("anonymous".to_string(), "anonymous@".to_string())
    } else {
        (profile.user.clone(), secret.unwrap_or_default().to_string())
    };

    let tls = match profile.protocol.as_str() {
        "ftps-implicit" => Tls::Implicit,
        // Port 990 is the classic implicit-FTPS port, and profiles saved
        // before `ftps-implicit` existed say implicit only by using it.
        // Keeping the heuristic keeps those profiles working; it is no
        // longer the *only* way to ask, which is what let an implicit
        // server on any other port be silently spoken to in the clear
        // (ticket 038).
        "ftps" if profile.port == 990 => Tls::Implicit,
        "ftps" => Tls::Explicit,
        _ => Tls::None,
    };

    let params = Params {
        profile: profile.clone(),
        fingerprint: Mutex::new(None),
        host: profile.host.clone(),
        port: profile.port,
        user,
        password,
        tls,
    };
    let ctrl = params.open()?;

    Ok(Arc::new(FtpProvider {
        key: profile.connection_key(),
        params,
        ctrl: Mutex::new(ctrl),
    }))
}

/// VFS provider bound to one FTP/FTPS server.
pub struct FtpProvider {
    key: String,
    params: Params,
    /// Shared control connection for short metadata operations.
    ctrl: Mutex<RustlsFtpStream>,
}

/// POSIX path string for the FTP protocol.
fn p(path: &Path) -> String {
    let s = super::provider_path::text(path);
    if s.is_empty() {
        "/".to_string()
    } else {
        s
    }
}

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

impl FtpProvider {
    /// Parses a directory listing into VFS entries.
    fn entries_of(&self, path: &str) -> AppResult<Vec<DirEntry>> {
        // Plain `LIST` hides dotfiles — the server runs the equivalent of
        // `ls`, not `ls -a`, so `.config` and friends are simply absent
        // from a remote pane while being right there on the server
        // (found by the protocol harness, ticket 038).
        //
        // `LIST -a` is the conventional way to ask for them and every
        // server the harness runs understands it, but it is an argument
        // smuggled through a path, so it is never the only attempt: a
        // server that errors, or that answers with nothing where the
        // plain form has entries, gets the plain form instead. Hiding
        // files would be a worse failure than not showing the hidden
        // ones.
        let lines = {
            let mut ctrl = self.ctrl.lock().unwrap();
            match ctrl.list(Some(&format!("-a {}", path))) {
                Ok(lines) if !lines.is_empty() => lines,
                // An empty answer costs one extra round trip on a
                // genuinely empty directory. That is the cheap half of
                // the trade.
                _ => ctrl
                    .list(Some(path))
                    .map_err(|e| AppError::from(format!("{}: {}", path, e)))?,
            }
        };

        Ok(lines
            .iter()
            .filter_map(|line| FtpFile::from_str(line).ok())
            .filter(|f| f.name() != "." && f.name() != "..")
            .map(|f| {
                let is_dir = f.is_directory();
                let modified = f
                    .modified()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                DirEntry {
                    hidden: f.name().starts_with('.'),
                    readonly: false,
                    executable: false,
                    name: f.name().to_string(),
                    is_dir,
                    // No target resolution here (TICKET-018): FTP has no
                    // `stat`, so the only way to learn whether a link
                    // points at a directory is to probe it with `CWD` —
                    // one extra round trip per link on a protocol that
                    // already pays for every one. The LIST line's own
                    // type character is what we report.
                    is_symlink: f.is_symlink(),
                    size: if is_dir { 0 } else { f.size() as u64 },
                    modified,
                    mode: None,
                }
            })
            .collect())
    }

    /// FTP has no `stat`: look the entry up in its parent listing.
    fn lookup(&self, path: &Path) -> Option<DirEntry> {
        let full = p(path);
        let (parent, name) = full.rsplit_once('/')?;
        let parent = if parent.is_empty() { "/" } else { parent };
        self.entries_of(parent)
            .ok()?
            .into_iter()
            .find(|e| e.name == name)
    }
}

impl Provider for FtpProvider {
    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        let mut info = crate::net::security::TransportInfo::connected(
            &self.params.profile,
            self.params.tls != Tls::None,
        );
        info.fingerprint = self.params.fingerprint.lock().ok().and_then(|f| f.clone());
        Some(info)
    }

    fn store_id(&self) -> String {
        // FTPS shares the store identity with FTP — same server, same files.
        format!("ftp:{}", self.key)
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        super::provider_path::validate(Path::new(path), super::Scheme::Ftp)?;
        let entries = self.entries_of(path)?;
        // Full address so the pane keeps its remote context.
        Ok(DirListing {
            path: super::location::remote_uri("ftp", &self.key, path)?,
            entries,
        })
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        // The connection root is always a directory.
        if p(path) == "/" {
            return Ok(Stat {
                is_dir: true,
                len: 0,
            });
        }
        match self.lookup(path) {
            Some(e) => Ok(Stat {
                is_dir: e.is_dir,
                len: e.size,
            }),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: nicht gefunden", p(path)),
            )),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        if super::provider_path::validate(path, super::Scheme::Ftp).is_err() {
            return false;
        }
        self.stat(path).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        if super::provider_path::validate(path, super::Scheme::Ftp).is_err() {
            return false;
        }
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        let base = p(path);
        let entries = self.entries_of(&base).map_err(to_io)?;
        Ok(entries
            .iter()
            .map(|e| Path::new(&base).join(&e.name))
            .collect())
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        // Dedicated connection: keeps long transfers off the shared
        // control connection (see the module comment).
        let mut ctrl = self.params.open().map_err(to_io)?;
        let stream = ctrl.retr_as_stream(p(path)).map_err(to_io)?;
        Ok(Box::new(FtpReader {
            ctrl: Some(ctrl),
            stream: Some(Box::new(stream)),
        }))
    }

    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        let mut ctrl = self.params.open().map_err(to_io)?;
        let stream = ctrl.put_with_stream(p(path)).map_err(to_io)?;
        Ok(Box::new(FtpWriter {
            ctrl: Some(ctrl),
            stream: Some(Box::new(stream)),
        }))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        let mut w = self.create_write(path)?;
        w.write_all(data)?;
        w.flush()
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        // FTP `MKD` is not recursive — create each missing component.
        let full = p(path);
        let mut acc = String::new();
        for part in full.split('/') {
            if part.is_empty() {
                continue;
            }
            acc.push('/');
            acc.push_str(part);
            if self.is_dir(Path::new(&acc)) {
                continue;
            }
            let mut ctrl = self.ctrl.lock().unwrap();
            // Ignore "already exists"; a real failure surfaces below.
            let _ = ctrl.mkdir(&acc);
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
        super::provider_path::validate(from, super::Scheme::Ftp)?;
        super::provider_path::validate(to, super::Scheme::Ftp)?;
        let mut ctrl = self.ctrl.lock().unwrap();
        ctrl.rename(p(from), p(to)).map_err(to_io)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        let mut ctrl = self.ctrl.lock().unwrap();
        ctrl.rm(p(path)).map_err(to_io)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Ftp)?;
        // `RMD` requires an empty directory — clear the contents first.
        for child in self.read_dir(path)? {
            if self.is_dir(&child) {
                self.remove_dir_all(&child)?;
            } else {
                self.remove_file(&child)?;
            }
        }
        let mut ctrl = self.ctrl.lock().unwrap();
        ctrl.rmdir(p(path)).map_err(to_io)
    }

    // ----- Failure-safe transfers (ticket 029) -----

    /// Commit is one `rename` inside the same connection.
    fn supports_staging(&self) -> bool {
        true
    }

    /// `RNFR`/`RNTO` has no portable replace semantics, so a
    /// replacement goes through the backup rename.
    fn rename_replaces(&self) -> bool {
        false
    }
}

// ---------- Transfer streams (own connection each) ----------

// The concrete `DataStream<RustlsStream>` type is not nameable
// (suppaftp keeps the module private), so the data streams are boxed —
// `finalize_retr_stream`/`finalize_put_stream` accept `impl Read`/
// `impl Write`, which a box satisfies.

/// Download stream; finalizes the FTP transfer when dropped.
struct FtpReader {
    ctrl: Option<RustlsFtpStream>,
    stream: Option<Box<dyn Read + Send>>,
}

impl Read for FtpReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.stream.as_mut() {
            Some(s) => s.read(buf),
            None => Ok(0),
        }
    }
}

impl Drop for FtpReader {
    fn drop(&mut self) {
        if let (Some(stream), Some(mut ctrl)) = (self.stream.take(), self.ctrl.take()) {
            let _ = ctrl.finalize_retr_stream(stream);
            let _ = ctrl.quit();
        }
    }
}

/// Upload stream. `flush` completes the transfer so errors surface to
/// the caller; `drop` is only the safety net for un-flushed streams.
struct FtpWriter {
    ctrl: Option<RustlsFtpStream>,
    stream: Option<Box<dyn Write + Send>>,
}

impl FtpWriter {
    /// Closes the data stream and reads the server's completion reply.
    fn finish(&mut self) -> io::Result<()> {
        let Some(stream) = self.stream.take() else {
            return Ok(()); // bereits abgeschlossen
        };
        let mut ctrl = match self.ctrl.take() {
            Some(c) => c,
            None => return Ok(()),
        };
        let res = ctrl.finalize_put_stream(stream).map_err(to_io);
        let _ = ctrl.quit();
        res
    }
}

impl Write for FtpWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.stream.as_mut() {
            Some(s) => s.write(buf),
            None => Err(io::Error::other("Upload bereits abgeschlossen")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(s) = self.stream.as_mut() {
            s.flush()?;
        }
        // Completing the upload here lets the transfer engine see errors.
        self.finish()
    }
}

impl Drop for FtpWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
