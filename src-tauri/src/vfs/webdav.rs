// ============================================================
// WebDAV provider (ticket 024).
//
// Implements the VFS `Provider` trait over WebDAV using reqwest_dav.
// The client is async, so — as with SFTP — every call is driven through
// the shared Tokio runtime in `net::session` via `block_on`.
//
// Navigation/attributes come from PROPFIND (`list`), transfers from
// GET/PUT, and the folder operations map to MKCOL/DELETE/MOVE.
// Downloads stream chunk by chunk; uploads are spooled to a temp file
// and then streamed to the server, so a large file never has to be
// held in memory.
//
// TLS: reqwest validates certificates by default — an invalid or
// self-signed certificate makes the connection fail (ticket
// requirement). `http` is only used when the profile's port is 80.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::net::session::{runtime, Profile};
use reqwest_dav::list_cmd::ListEntity;
use reqwest_dav::{Auth, Client, ClientBuilder, Depth};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Opens a WebDAV connection and returns the ready provider.
/// The credentials are verified with a PROPFIND on the start path, so
/// a wrong password or an unreachable host fails here, not later.
pub fn connect(profile: &Profile, secret: Option<&str>) -> Result<Arc<dyn Provider>, String> {
    let auth = match profile.auth.as_str() {
        "anonymous" => Auth::Anonymous,
        // `digest` selects Digest; everything else uses Basic.
        "digest" => Auth::Digest(profile.user.clone(), secret.unwrap_or_default().to_string()),
        _ => Auth::Basic(profile.user.clone(), secret.unwrap_or_default().to_string()),
    };

    // The host may carry an explicit scheme (`http://dav.example`) — that
    // wins, so a plain-HTTP server on a non-standard port is reachable.
    // Otherwise: HTTP only on the classic port 80, HTTPS everywhere else.
    let (url_scheme, bare_host) = match profile.host.split_once("://") {
        Some((scheme, rest)) => (scheme, rest),
        None => (
            if profile.port == 80 { "http" } else { "https" },
            profile.host.as_str(),
        ),
    };
    let host = format!("{}://{}:{}", url_scheme, bare_host.trim_end_matches('/'), profile.port);

    let client = ClientBuilder::new()
        .set_host(host.clone())
        .set_auth(auth)
        .build()
        .map_err(|e| format!("WebDAV-Client konnte nicht erstellt werden: {}", e))?;

    let provider = WebdavProvider {
        key: profile.connection_key(),
        base: host,
        client,
    };

    // Probe the start path so auth/TLS errors surface on connect.
    let start = if profile.start_path.is_empty() {
        "/"
    } else {
        &profile.start_path
    };
    provider
        .list(start)
        .map_err(|e| format!("Verbindung fehlgeschlagen: {}", e))?;

    Ok(Arc::new(provider))
}

/// VFS provider bound to one WebDAV server.
pub struct WebdavProvider {
    key: String,
    /// Absolute server base (`scheme://host:port`) — needed for the
    /// `Destination` header of MOVE.
    base: String,
    client: Client,
}

/// Path string for the WebDAV protocol (always POSIX-style).
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

/// Decodes the `%XX` escapes of an href segment.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Last path segment of an href (the entry name), percent-decoded.
fn name_of(href: &str) -> String {
    let trimmed = href.trim_end_matches('/');
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    percent_decode(last)
}

/// Normalizes a path/href for comparison (no trailing slash, decoded).
fn norm(s: &str) -> String {
    let d = percent_decode(s);
    let t = d.trim_end_matches('/');
    if t.is_empty() {
        "/".to_string()
    } else {
        t.to_string()
    }
}

impl WebdavProvider {
    /// PROPFIND with depth 0 — metadata of the entry itself.
    fn stat_entity(&self, path: &str) -> Result<ListEntity, String> {
        let items = runtime()
            .block_on(self.client.list(path, Depth::Number(0)))
            .map_err(|e| format!("{}: {}", path, e))?;
        items
            .into_iter()
            .next()
            .ok_or_else(|| format!("{}: nicht gefunden", path))
    }
}

impl Provider for WebdavProvider {
    fn store_id(&self) -> String {
        format!("webdav:{}", self.key)
    }

    fn list(&self, path: &str) -> Result<DirListing, String> {
        let items = runtime()
            .block_on(self.client.list(path, Depth::Number(1)))
            .map_err(|e| format!("{}: {}", path, e))?;

        let self_key = norm(path);
        let mut entries = Vec::new();
        for item in items {
            let (href, is_dir, size, modified) = match item {
                ListEntity::File(f) => (
                    f.href,
                    false,
                    f.content_length.max(0) as u64,
                    f.last_modified.timestamp().max(0) as u64,
                ),
                ListEntity::Folder(d) => {
                    (d.href, true, 0u64, d.last_modified.timestamp().max(0) as u64)
                }
            };
            // PROPFIND depth 1 includes the collection itself — skip it.
            if norm(&href).ends_with(&self_key) {
                continue;
            }
            let name = name_of(&href);
            if name.is_empty() {
                continue;
            }
            entries.push(DirEntry {
                hidden: name.starts_with('.'),
                readonly: false,
                executable: false,
                name,
                is_dir,
                is_symlink: false,
                size,
                modified,
                mode: None,
            });
        }

        // Full address so the pane keeps its remote context.
        Ok(DirListing {
            path: format!("webdav://{}{}", self.key, path),
            entries,
        })
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let entity = self.stat_entity(&p(path)).map_err(to_io)?;
        Ok(match entity {
            ListEntity::File(f) => Stat {
                is_dir: false,
                len: f.content_length.max(0) as u64,
            },
            ListEntity::Folder(_) => Stat {
                is_dir: true,
                len: 0,
            },
        })
    }

    fn exists(&self, path: &Path) -> bool {
        self.stat_entity(&p(path)).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let base = p(path);
        let listing = self.list(&base).map_err(to_io)?;
        Ok(listing
            .entries
            .iter()
            .map(|e| Path::new(&base).join(&e.name))
            .collect())
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        let response = runtime()
            .block_on(self.client.get(&p(path)))
            .map_err(to_io)?;
        Ok(Box::new(WebdavReader {
            response: Some(response),
            buf: Vec::new(),
            pos: 0,
        }))
    }

    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(WebdavWriter::new(self.client.clone(), p(path))?))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        runtime()
            .block_on(self.client.put(&p(path), data.to_vec()))
            .map_err(to_io)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        // MKCOL creates a single collection — walk the components.
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
            // Ignore "already exists"; the final check reports real failures.
            let _ = runtime().block_on(self.client.mkcol(&acc));
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

    /// MOVE. Issued directly rather than via `Client::mv`, because that
    /// helper sends a **path-only** `Destination` header while RFC 4918
    /// (and Apache mod_dav) require an absolute URI — servers answer the
    /// short form with "400 Bad Request".
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        use reqwest_dav::re_exports::reqwest::Method;

        let destination = format!("{}{}", self.base, p(to));
        runtime().block_on(async {
            let method = Method::from_bytes(b"MOVE").map_err(to_io)?;
            let response = self
                .client
                .start_request(method, &p(from))
                .await
                .map_err(to_io)?
                .header("Destination", destination)
                // Match the local semantics: never clobber an existing target.
                .header("Overwrite", "F")
                .send()
                .await
                .map_err(to_io)?;
            if response.status().is_success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "MOVE {} → {}: HTTP {}",
                    p(from),
                    p(to),
                    response.status()
                )))
            }
        })
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        runtime()
            .block_on(self.client.delete(&p(path)))
            .map_err(to_io)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        // DELETE on a collection removes it recursively.
        runtime()
            .block_on(self.client.delete(&p(path)))
            .map_err(to_io)
    }
}

// ---------- Transfer streams ----------

/// Streaming download: pulls one chunk at a time off the response body.
struct WebdavReader {
    response: Option<reqwest_dav::re_exports::reqwest::Response>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for WebdavReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        // Refill from the next body chunk when the buffer is drained.
        while self.pos >= self.buf.len() {
            let Some(resp) = self.response.as_mut() else {
                return Ok(0);
            };
            match runtime().block_on(resp.chunk()).map_err(to_io)? {
                Some(chunk) if chunk.is_empty() => continue,
                Some(chunk) => {
                    self.buf = chunk.to_vec();
                    self.pos = 0;
                }
                None => {
                    self.response = None;
                    return Ok(0);
                }
            }
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Upload: WebDAV PUT needs a body, so the data is spooled to a temp
/// file and streamed to the server on `flush` (the transfer engine
/// calls it, so upload errors reach the caller); `drop` is the safety
/// net for un-flushed streams.
struct WebdavWriter {
    client: Client,
    path: String,
    spool: Option<std::fs::File>,
    spool_path: PathBuf,
    finished: bool,
}

impl WebdavWriter {
    fn new(client: Client, path: String) -> io::Result<Self> {
        let spool_path = std::env::temp_dir().join(format!(
            "hortcmd-dav-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let spool = std::fs::File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&spool_path)?;
        Ok(Self {
            client,
            path,
            spool: Some(spool),
            spool_path,
            finished: false,
        })
    }

    /// Uploads the spooled content and removes the temp file.
    fn finish(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let Some(mut file) = self.spool.take() else {
            return Ok(());
        };
        file.flush()?;
        file.seek(SeekFrom::Start(0))?;
        drop(file);

        let path = self.path.clone();
        let client = self.client.clone();
        let spool_path = self.spool_path.clone();
        let result = runtime().block_on(async move {
            // Stream the spool file instead of buffering it in memory.
            let f = tokio::fs::File::open(&spool_path).await.map_err(to_io)?;
            let body = reqwest_dav::re_exports::reqwest::Body::wrap_stream(
                tokio_util::io::ReaderStream::new(f),
            );
            client.put(&path, body).await.map_err(to_io)
        });
        let _ = std::fs::remove_file(&self.spool_path);
        result
    }
}

impl Write for WebdavWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.spool.as_mut() {
            Some(f) => f.write(buf),
            None => Err(io::Error::other("Upload bereits abgeschlossen")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.finish()
    }
}

impl Drop for WebdavWriter {
    fn drop(&mut self) {
        let _ = self.finish();
        let _ = std::fs::remove_file(&self.spool_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn href_name_is_decoded() {
        assert_eq!(name_of("/dav/files/Mein%20Ordner/"), "Mein Ordner");
        assert_eq!(name_of("/dav/a/b/report.pdf"), "report.pdf");
        assert_eq!(name_of("/dav/files/%C3%A4%C3%B6.txt"), "äö.txt");
    }

    #[test]
    fn norm_strips_trailing_slash() {
        assert_eq!(norm("/a/b/"), "/a/b");
        assert_eq!(norm("/"), "/");
        assert_eq!(norm("/a%20b/"), "/a b");
    }
}
