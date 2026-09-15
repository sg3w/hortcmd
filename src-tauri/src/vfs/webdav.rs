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
// requirement). HTTP requires an explicit scheme and both plaintext opt-ins.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{code, AppError, AppResult};
use crate::net::session::{runtime, Profile};
use reqwest_dav::list_cmd::ListEntity;
use reqwest_dav::{Auth, Client, ClientBuilder, Depth};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;

/// Opens a WebDAV connection and returns the ready provider.
/// A reqwest client carrying the extra trust anchors, or `None` when
/// none are configured and `reqwest_dav`'s own default client will do.
fn http_agent(
    profile: &Profile,
) -> AppResult<(
    reqwest_dav::re_exports::reqwest::Client,
    crate::net::tls_config::TlsEvidence,
)> {
    use reqwest_dav::re_exports::reqwest::{redirect, Client};
    let (config, evidence) = crate::net::tls_config::config(profile)?;
    let agent = Client::builder()
        .redirect(redirect::Policy::none())
        .use_preconfigured_tls(config)
        .build()
        .map_err(|error| AppError::new(code::NET_TLS).op("tls-client").detail(error))?;
    Ok((agent, evidence))
}

/// The credentials are verified with a PROPFIND on the start path, so
/// a wrong password or an unreachable host fails here, not later.
pub fn connect(profile: &Profile, secret: Option<&str>) -> AppResult<Arc<dyn Provider>> {
    crate::net::security::check(profile)?;
    let auth = match profile.auth.as_str() {
        "anonymous" => Auth::Anonymous,
        // `digest` selects Digest; everything else uses Basic.
        "digest" => Auth::Digest(profile.user.clone(), secret.unwrap_or_default().to_string()),
        _ => Auth::Basic(profile.user.clone(), secret.unwrap_or_default().to_string()),
    };

    let (url_scheme, bare_host) = crate::net::security::webdav_endpoint(profile)?;
    let bare_host = if bare_host.contains(':') && !bare_host.starts_with('[') {
        format!("[{bare_host}]")
    } else {
        bare_host.to_owned()
    };
    let host = format!(
        "{}://{}:{}",
        url_scheme,
        bare_host.trim_end_matches('/'),
        profile.port
    );

    let (agent, evidence) = http_agent(profile)?;
    let builder = ClientBuilder::new()
        .set_host(host.clone())
        .set_auth(auth)
        .set_agent(agent);
    let client = builder.build().map_err(|e| {
        AppError::from(format!("WebDAV-Client konnte nicht erstellt werden: {}", e))
    })?;

    let mut provider = WebdavProvider {
        security: crate::net::security::TransportInfo::connected(profile, url_scheme == "https"),
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
    provider.list(start).map_err(|e| {
        evidence
            .failure
            .lock()
            .ok()
            .and_then(|f| f.clone())
            .unwrap_or_else(|| AppError::from(format!("Verbindung fehlgeschlagen: {}", e)))
    })?;

    provider.security.fingerprint = evidence.fingerprint.lock().ok().and_then(|f| f.clone());
    Ok(Arc::new(provider))
}

/// VFS provider bound to one WebDAV server.
pub struct WebdavProvider {
    security: crate::net::security::TransportInfo,
    key: String,
    /// Absolute server base (`scheme://host:port`) — needed for the
    /// `Destination` header of MOVE.
    base: String,
    client: Client,
}

/// Path string for the WebDAV protocol (always POSIX-style).
fn p(path: &crate::vfs::StreamPath) -> String {
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

/// Percent-encodes a path for a request URI, keeping `/` as the
/// separator.
///
/// Everything outbound goes through this. A WebDAV path is part of a
/// URL, and a name is not: `100%-done.txt` is an invalid escape
/// sequence that Apache answers with "400 Bad Request", `a#b.txt` cuts
/// the request short at the fragment, and `a?b.txt` turns the rest into
/// a query. The file listing shows all three — they arrive as hrefs and
/// are decoded by [`percent_decode`] — so before this every such file
/// was visible and untouchable (found by the protocol harness,
/// ticket 038).
///
/// Unreserved characters (RFC 3986 §2.3) are left alone; everything
/// else, including every byte of a non-ASCII name, is escaped.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.as_bytes() {
        match byte {
            b'/' => out.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

/// The encoded form of a path, for a request.
fn e(path: &crate::vfs::StreamPath) -> String {
    encode_path(&p(path))
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
    let trimmed = href.strip_suffix('/').unwrap_or(href);
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    percent_decode(last)
}

/// Decode an href for comparison, removing only its directory delimiter.
fn norm(s: &str) -> String {
    let path = s.split_once("://").map_or(s, |(_, authority)| {
        authority.find('/').map_or("/", |index| &authority[index..])
    });
    let d = percent_decode(path);
    let t = d.strip_suffix('/').unwrap_or(&d);
    if t.is_empty() {
        "/".to_string()
    } else {
        t.to_string()
    }
}

impl WebdavProvider {
    /// PROPFIND with depth 0 — metadata of the entry itself.
    fn stat_entity(&self, path: &str) -> AppResult<ListEntity> {
        let items = runtime()
            .block_on(self.client.list(&encode_path(path), Depth::Number(0)))
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
        items
            .into_iter()
            .next()
            .ok_or_else(|| AppError::from(format!("{}: nicht gefunden", path)))
    }
}

impl Provider for WebdavProvider {
    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        Some(self.security.clone())
    }
    fn store_id(&self) -> String {
        format!("webdav:{}", self.key)
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        super::provider_path::validate(
            &crate::vfs::StreamPath::remote(path)?,
            super::Scheme::Webdav,
        )?;
        let items = runtime()
            .block_on(self.client.list(&encode_path(path), Depth::Number(1)))
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;

        let self_key = norm(&encode_path(path));
        let mut entries = Vec::new();
        for item in items {
            let (href, is_dir, size, modified) = match item {
                ListEntity::File(f) => (
                    f.href,
                    false,
                    f.content_length.max(0) as u64,
                    f.last_modified.timestamp().max(0) as u64,
                ),
                ListEntity::Folder(d) => (
                    d.href,
                    true,
                    0u64,
                    d.last_modified.timestamp().max(0) as u64,
                ),
            };
            // PROPFIND depth 1 includes the collection itself — skip it.
            if norm(&href) == self_key {
                continue;
            }
            let name = name_of(&href);
            if name.is_empty() {
                return Err(AppError::from(
                    "empty WebDAV entry name cannot be traversed",
                ));
            }
            entries.push(DirEntry {
                hidden: name.starts_with('.'),
                readonly: false,
                executable: false,
                name,
                is_dir,
                // WebDAV has no symlink concept — a collection is a
                // collection — so there is nothing to resolve (TICKET-018).
                is_symlink: false,
                size,
                modified,
                mode: None,
            });
        }

        // Full address so the pane keeps its remote context.
        Ok(DirListing {
            path: super::location::remote_uri("webdav", &self.key, path)?,
            entries,
        })
    }

    fn stat(&self, path: &crate::vfs::StreamPath) -> io::Result<Stat> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
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

    fn exists(&self, path: &crate::vfs::StreamPath) -> bool {
        if super::provider_path::validate(path, super::Scheme::Webdav).is_err() {
            return false;
        }
        self.stat_entity(&p(path)).is_ok()
    }

    fn is_dir(&self, path: &crate::vfs::StreamPath) -> bool {
        if super::provider_path::validate(path, super::Scheme::Webdav).is_err() {
            return false;
        }
        self.stat(path).map(|s| s.is_dir).unwrap_or(false)
    }

    fn read_dir(&self, path: &crate::vfs::StreamPath) -> io::Result<Vec<crate::vfs::StreamPath>> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        let base = p(path);
        let listing = self.list(&base).map_err(to_io)?;
        listing
            .entries
            .iter()
            .map(|e| path.join(&e.name))
            .collect::<io::Result<Vec<_>>>()
    }

    fn open_read(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Read + Send>> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        let response = runtime()
            .block_on(self.client.get(&e(path)))
            .map_err(to_io)?;
        Ok(Box::new(WebdavReader {
            response: Some(response),
            buf: Vec::new(),
            pos: 0,
        }))
    }

    fn create_write(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Write + Send>> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        Ok(Box::new(WebdavWriter::new(self.client.clone(), e(path))?))
    }

    fn write(&self, path: &crate::vfs::StreamPath, data: &[u8]) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        runtime()
            .block_on(self.client.put(&e(path), data.to_vec()))
            .map_err(to_io)
    }

    fn create_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        // MKCOL creates a single collection — walk the components.
        let full = p(path);
        for acc in super::provider_path::prefixes(path)? {
            if self.is_dir(&crate::vfs::StreamPath::remote(&acc).map_err(io::Error::other)?) {
                continue;
            }
            // Ignore "already exists"; the final check reports real failures.
            let _ = runtime().block_on(self.client.mkcol(&encode_path(&acc)));
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
    fn rename(&self, from: &crate::vfs::StreamPath, to: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(from, super::Scheme::Webdav)?;
        super::provider_path::validate(to, super::Scheme::Webdav)?;
        use reqwest_dav::re_exports::reqwest::Method;

        let destination = format!("{}{}", self.base, e(to));
        runtime().block_on(async {
            let method = Method::from_bytes(b"MOVE").map_err(to_io)?;
            let response = self
                .client
                .start_request(method, &e(from))
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

    fn remove_file(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        runtime()
            .block_on(self.client.delete(&e(path)))
            .map_err(to_io)
    }

    fn remove_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        // DELETE on a collection removes it recursively.
        runtime()
            .block_on(self.client.delete(&e(path)))
            .map_err(to_io)
    }

    // ----- Failure-safe transfers (ticket 029) -----

    /// Commit is one `rename` inside the same connection.
    fn supports_staging(&self) -> bool {
        true
    }

    /// `rename` deliberately sends `Overwrite: F`, so a
    /// replacement goes through the backup rename.
    fn rename_replaces(&self) -> bool {
        false
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

    /// Everything that is not an unreserved character is escaped, and
    /// the separators survive. The `%` case is the one that used to make
    /// a listed file unreachable: `100%-done.txt` is not a valid escape
    /// sequence, and Apache answers the request with 400 (ticket 038).
    #[test]
    fn request_paths_are_percent_encoded() {
        assert_eq!(encode_path("/a/b/report.pdf"), "/a/b/report.pdf");
        assert_eq!(encode_path("/tree/100%-done.txt"), "/tree/100%25-done.txt");
        assert_eq!(
            encode_path("/tree/name with spaces.txt"),
            "/tree/name%20with%20spaces.txt"
        );
        assert_eq!(encode_path("/tree/hash#tag.txt"), "/tree/hash%23tag.txt");
        assert_eq!(encode_path("/tree/what?.txt"), "/tree/what%3F.txt");
        assert_eq!(encode_path("/tree/äö.txt"), "/tree/%C3%A4%C3%B6.txt");
        // Unreserved characters stay readable — an escaped path nobody
        // can read in a log is a worse debugging experience for nothing.
        assert_eq!(encode_path("/a-b_c.d~e"), "/a-b_c.d~e");
    }

    /// Encoding and decoding are the two halves of one contract: a name
    /// that came back from a listing has to address the same entry.
    #[test]
    fn encoding_round_trips_through_the_decoder() {
        for name in [
            "100%-done.txt",
            "name with spaces.txt",
            "hash#tag.txt",
            "semi;colon.txt",
            "quote'single.txt",
            "Ümlaut-Grüße.txt",
            "日本語.txt",
        ] {
            let encoded = encode_path(&format!("/tree/{}", name));
            assert_eq!(name_of(&encoded), name, "round trip of {}", name);
        }
    }

    #[test]
    fn norm_strips_trailing_slash() {
        assert_eq!(norm("/a/b/"), "/a/b");
        assert_eq!(norm("/"), "/");
        assert_eq!(norm("/a%20b/"), "/a b");
        assert_eq!(norm("/a//"), "/a/");
        assert_eq!(norm("https://host/a//"), "/a/");
        assert_ne!(norm("/a/a"), norm("/a"));
        assert_ne!(norm("/a//"), norm("/a/"));
        assert_eq!(name_of("/a//"), "");
    }
}
