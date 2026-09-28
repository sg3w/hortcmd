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

use super::listing::{self, ListingBudget};
use super::{Provider, Stat};
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{code, AppError, AppResult};
use crate::net::session::{runtime, Profile};
use reqwest_dav::list_cmd::ListEntity;
use reqwest_dav::re_exports::reqwest::Response;
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
        .connect_timeout(super::network_timeout(profile))
        .timeout(super::network_timeout(profile))
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
        health_path: if profile.start_path.is_empty() {
            "/".into()
        } else {
            profile.start_path.clone()
        },
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
    health_path: String,
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

/// The collection form of a request path: WebDAV addresses a collection
/// with a trailing slash.
///
/// Apache `mod_dav` answers a collection addressed without it with `301`
/// and a `Location` that only adds the slash — and automatic redirect
/// following stays off (ticket 047), so every request whose target is
/// known to be a collection is addressed correctly from the start
/// (ticket 378).
fn collection(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

/// Whether a `3xx` answer may be retried, and against which path.
///
/// The only redirect hortcmd follows is the collection delimiter on the
/// very same resource: same scheme, same host, same port, and a target
/// path that is the request path plus a trailing slash. Anything else —
/// another origin, a scheme downgrade, a different port, an unrelated
/// path — returns `None`, so no credential is ever replayed elsewhere.
fn slash_redirect(base: &str, request: &str, location: &str) -> Option<String> {
    use reqwest_dav::re_exports::url::Url;

    let from = Url::parse(&format!(
        "{}/{}",
        base.trim_end_matches('/'),
        request.trim_start_matches('/')
    ))
    .ok()?;
    let to = from.join(location).ok()?;
    let same_origin = to.scheme() == from.scheme()
        && to.host_str() == from.host_str()
        && to.port_or_known_default() == from.port_or_known_default();
    if !same_origin || to.query().is_some() || to.fragment().is_some() {
        return None;
    }
    if to.path() != collection(from.path()) {
        return None;
    }
    Some(collection(request))
}

/// Whether a `3xx` that only adds the collection delimiter is followed.
#[derive(Clone, Copy)]
enum Redirect {
    /// Retry the request once against the collection form.
    Follow,
    /// Report the redirect — this operation's target is not a collection.
    Refuse,
}

/// A redirect that cannot be resolved as a collection delimiter. Reported
/// as a protocol error instead of leaking a status mismatch as
/// `internal.unexpected`.
fn redirect_error(path: &str, status: u16, location: &str) -> AppError {
    AppError::new(code::NET_PROTOCOL)
        .op("webdav-redirect")
        .provider("webdav")
        .with("path", path)
        .with("status", status)
        .detail(format!("HTTP {status} → {location}"))
}

impl WebdavProvider {
    /// Sends one request and, for a `3xx`, retries it once against a
    /// `Location` that only adds the collection delimiter (ticket 378).
    ///
    /// `send` is called with the request path, so the retry rebuilds the
    /// request — headers and body included — for the corrected target.
    async fn send<F, Fut>(&self, path: &str, redirect: Redirect, send: F) -> AppResult<Response>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<Response, reqwest_dav::Error>>,
    {
        let first = send(path.to_string())
            .await
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
        if !first.status().is_redirection() {
            return Ok(first);
        }
        let status = first.status().as_u16();
        let location = first
            .headers()
            .get(reqwest_dav::re_exports::reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let retry = match redirect {
            Redirect::Follow => slash_redirect(&self.base, path, &location),
            Redirect::Refuse => None,
        }
        .ok_or_else(|| redirect_error(path, status, &location))?;
        let second = send(retry.clone())
            .await
            .map_err(|e| AppError::from(format!("{}: {}", retry, e)))?;
        if second.status().is_redirection() {
            let status = second.status().as_u16();
            return Err(redirect_error(&retry, status, &location));
        }
        Ok(second)
    }

    /// PROPFIND, driven from the raw response: `reqwest_dav`'s own helper
    /// turns a redirect into a bare status mismatch, which leaves no way
    /// to read the `Location` (ticket 378).
    fn propfind(&self, path: &str, depth: Depth, redirect: Redirect) -> AppResult<Vec<ListEntity>> {
        runtime().block_on(self.propfind_async(path, depth, redirect))
    }

    async fn propfind_async(
        &self,
        path: &str,
        depth: Depth,
        redirect: Redirect,
    ) -> AppResult<Vec<ListEntity>> {
        let response = self
            .send(path, redirect, |target| {
                let depth = depth.clone();
                async move { self.client.list_raw(&target, depth).await }
            })
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::from(format!(
                "{}: PROPFIND HTTP {}",
                path,
                status.as_u16()
            )));
        }
        let body = response
            .text()
            .await
            .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
        let multistatus: reqwest_dav::list_cmd::ListMultiStatus =
            reqwest_dav::re_exports::serde_xml_rs::from_str(&body)
                .map_err(|e| AppError::from(format!("{}: {}", path, e)))?;
        multistatus
            .responses
            .into_iter()
            .map(|response| {
                ListEntity::try_from(response)
                    .map_err(|e| AppError::from(format!("{}: {}", path, e)))
            })
            .collect()
    }

    /// One redirect-aware request whose body is not read — MKCOL, DELETE
    /// and MOVE all answer with a status only.
    async fn command<F, Fut>(
        &self,
        label: &str,
        path: &str,
        redirect: Redirect,
        send: F,
    ) -> io::Result<()>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<Response, reqwest_dav::Error>>,
    {
        let response = self.send(path, redirect, send).await.map_err(to_io)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{} {}: HTTP {}",
                label,
                path,
                response.status()
            )))
        }
    }

    /// PROPFIND with depth 0 — metadata of the entry itself.
    /// Entries of one collection, charged to `budget` while the PROPFIND
    /// answer is walked so a fabricated multistatus stops here (ticket 321).
    fn entries_of(&self, path: &str, budget: &mut ListingBudget) -> AppResult<Vec<DirEntry>> {
        super::provider_path::validate(
            &crate::vfs::StreamPath::remote(path)?,
            super::Scheme::Webdav,
        )?;
        // Depth 1 always addresses a collection, so ask for it as one.
        let items = self.propfind(
            &collection(&encode_path(path)),
            Depth::Number(1),
            Redirect::Follow,
        )?;

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
            budget.charge(&name)?;
            if name == "." || name == ".." {
                continue;
            }
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

        Ok(entries)
    }

    fn stat_entity(&self, path: &str) -> AppResult<ListEntity> {
        // Depth 0 may address either kind, so the redirect decides.
        let items = self.propfind(&encode_path(path), Depth::Number(0), Redirect::Follow)?;
        items
            .into_iter()
            .next()
            .ok_or_else(|| AppError::from(format!("{}: nicht gefunden", path)))
    }
}

impl Provider for WebdavProvider {
    fn health_check(&self, timeout: std::time::Duration) -> io::Result<super::Health> {
        runtime().block_on(async {
            tokio::time::timeout(
                timeout,
                self.propfind_async(
                    &encode_path(&self.health_path),
                    Depth::Number(0),
                    Redirect::Follow,
                ),
            )
            .await
            .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
            .map_err(to_io)?;
            Ok(super::Health::Alive)
        })
    }

    fn transport_security(&self) -> Option<crate::net::security::TransportInfo> {
        Some(self.security.clone())
    }
    fn store_id(&self) -> String {
        format!("webdav:{}", self.key)
    }

    fn list(&self, path: &str) -> AppResult<DirListing> {
        let entries = self.entries_of(path, &mut ListingBudget::default())?;
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
        self.read_dir_bounded(path, listing::MAX_ENTRIES)
    }

    fn read_dir_bounded(
        &self,
        path: &crate::vfs::StreamPath,
        limit: usize,
    ) -> io::Result<Vec<crate::vfs::StreamPath>> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        let base = p(path);
        let entries = self
            .entries_of(&base, &mut ListingBudget::new(limit))
            .map_err(to_io)?;
        entries
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
            // MKCOL creates a collection, so it is addressed as one.
            let _ = runtime().block_on(self.command(
                "MKCOL",
                &collection(&encode_path(&acc)),
                Redirect::Follow,
                |target| async move { self.client.mkcol_raw(&target).await },
            ));
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

        let base = self.base.clone();
        let target = e(to);
        runtime().block_on(self.command(
            "MOVE",
            &e(from),
            // A directory is redirected to its collection form; the
            // destination then names a collection too.
            Redirect::Follow,
            |source| {
                let destination = if source.ends_with('/') {
                    format!("{}{}", base, collection(&target))
                } else {
                    format!("{}{}", base, target)
                };
                async move {
                    let method = Method::from_bytes(b"MOVE")?;
                    Ok(self
                        .client
                        .start_request(method, &source)
                        .await?
                        .header("Destination", destination)
                        // Match the local semantics: never clobber an
                        // existing target.
                        .header("Overwrite", "F")
                        .send()
                        .await?)
                }
            },
        ))
    }

    fn remove_file(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        // A redirect to the collection form would mean this is a
        // collection — DELETE there removes a whole tree, so it is
        // refused rather than followed.
        runtime().block_on(self.command(
            "DELETE",
            &e(path),
            Redirect::Refuse,
            |target| async move { self.client.delete_raw(&target).await },
        ))
    }

    fn remove_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        super::provider_path::validate(path, super::Scheme::Webdav)?;
        // DELETE on a collection removes it recursively.
        runtime().block_on(self.command(
            "DELETE",
            &collection(&e(path)),
            Redirect::Follow,
            |target| async move { self.client.delete_raw(&target).await },
        ))
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
/// calls it, so upload errors reach the caller). A stream that is dropped
/// without a `flush` is an abandoned transfer — cancel drops the writer and
/// then deletes the destination — so `drop` discards the spool instead of
/// uploading it (ticket 383). Every other writer here streams its bytes as
/// they are written, so only this one can withhold a whole upload.
struct WebdavWriter {
    client: Client,
    path: String,
    spool: Option<std::fs::File>,
    spool_path: PathBuf,
    finished: bool,
}

/// Reserve a private spool exclusively; never truncate a collision or follow a
/// pre-existing symlink. Bound retries even if the temporary directory is hostile.
fn create_spool(
    mut candidate: impl FnMut() -> io::Result<PathBuf>,
) -> io::Result<(PathBuf, std::fs::File)> {
    for _ in 0..16 {
        let path = candidate()?;
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "upload spool collisions",
    ))
}

impl WebdavWriter {
    fn new(client: Client, path: String) -> io::Result<Self> {
        let (spool_path, spool) = create_spool(|| {
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(io::Error::other)?;
            Ok(std::env::temp_dir().join(format!("hortcmd-dav-{}", hex::encode(random))))
        })?;
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

        let path = self.path.clone();
        let client = self.client.clone();
        let result = runtime().block_on(async move {
            // Stream the spool file instead of buffering it in memory.
            let f = tokio::fs::File::from_std(file);
            let body = reqwest_dav::re_exports::reqwest::Body::wrap_stream(
                tokio_util::io::ReaderStream::new(f),
            );
            client.put(&path, body).await.map_err(to_io)
        });
        let _ = std::fs::remove_file(&self.spool_path);
        result
    }

    /// Throws the spool away without a `PUT`. Nothing was published, so
    /// there is nothing to report to a caller that is no longer listening.
    fn abandon(&mut self) {
        self.finished = true;
        self.spool = None;
        let _ = std::fs::remove_file(&self.spool_path);
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
        self.abandon();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_filters_literal_and_encoded_dot_entries_from_propfind() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let response = [
            "/root/", "/root/./", "/root/../", "/root/%2e/", "/root/%2E%2E/",
            "http://dav.test/root/..", "/root/.hidden/", "/root/normal/", "/root/%252e/",
        ].into_iter().map(|href| format!(
            "<D:response><D:href>{href}</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>"
        )).collect::<String>();
        let body = format!(
            "<?xml version=\"1.0\"?><D:multistatus xmlns:D=\"DAV:\">{response}</D:multistatus>"
        );
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            // Depth 1 addresses the collection with its delimiter (ticket 378).
            assert!(line.starts_with("PROPFIND /root/ "), "{line}");
            let mut length = 0;
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            reader.read_exact(&mut vec![0; length]).unwrap();
            write!(socket, "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let profile = Profile {
            id: "webdav-dot-test".into(),
            name: "test".into(),
            protocol: "webdav".into(),
            host: "http://127.0.0.1".into(),
            port,
            user: "test".into(),
            auth: "anonymous".into(),
            key_path: None,
            start_path: "/root".into(),
            security: Default::default(),
            monitor: None,
        };
        let base = format!("http://127.0.0.1:{port}");
        let agent = reqwest_dav::re_exports::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let client = ClientBuilder::new()
            .set_host(base.clone())
            .set_auth(Auth::Anonymous)
            .set_agent(agent)
            .build()
            .unwrap();
        let provider = WebdavProvider {
            health_path: "/root".into(),
            security: crate::net::security::TransportInfo::connected(&profile, false),
            key: profile.connection_key(),
            base,
            client,
        };
        let listing = provider.list("/root").unwrap();
        server.join().unwrap();
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [".hidden", "normal", "%2e"]
        );
        assert!(listing.entries[0].hidden);
        assert!(listing.entries.iter().all(|entry| entry.is_dir));
    }

    /// Cancel drops the writer without flushing it. Nothing may reach the
    /// server then — uploading the partial spool and deleting it afterwards is
    /// exactly what ticket 383 removed — while a flushed stream still PUTs its
    /// whole body.
    #[test]
    fn a_dropped_upload_sends_nothing_and_a_flushed_one_sends_everything() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let agent = reqwest_dav::re_exports::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let client = ClientBuilder::new()
            .set_host(format!("http://127.0.0.1:{port}"))
            .set_auth(Auth::Anonymous)
            .set_agent(agent)
            .build()
            .unwrap();

        let mut abandoned = WebdavWriter::new(client.clone(), "/cancelled.bin".into()).unwrap();
        abandoned.write_all(&[7u8; 4096]).unwrap();
        let spool = abandoned.spool_path.clone();
        assert!(spool.exists());
        drop(abandoned);
        assert!(
            !spool.exists(),
            "the spool file survived the abandoned upload"
        );
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock,
            "a cancelled upload contacted the server"
        );
        listener.set_nonblocking(false).unwrap();

        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request = line.clone();
            let mut length = 0;
            let mut chunked = false;
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
                chunked |= lower.starts_with("transfer-encoding:") && lower.contains("chunked");
            }
            let mut body = Vec::new();
            if chunked {
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    let size = usize::from_str_radix(line.trim(), 16).unwrap();
                    let mut chunk = vec![0u8; size + 2];
                    reader.read_exact(&mut chunk).unwrap();
                    if size == 0 {
                        break;
                    }
                    body.extend_from_slice(&chunk[..size]);
                }
            } else {
                body.resize(length, 0);
                reader.read_exact(&mut body).unwrap();
            }
            write!(
                socket,
                "HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            (request, body)
        });

        let mut committed = WebdavWriter::new(client, "/kept.bin".into()).unwrap();
        let payload = vec![9u8; 4096];
        committed.write_all(&payload).unwrap();
        let spool = committed.spool_path.clone();
        committed.flush().unwrap();
        drop(committed);
        let (request, body) = server.join().unwrap();
        assert!(request.starts_with("PUT /kept.bin "), "{request}");
        assert_eq!(body, payload);
        assert!(!spool.exists());
    }

    #[test]
    fn spool_collisions_preserve_existing_files_and_retry() {
        let root =
            std::env::temp_dir().join(format!("hortcmd-spool-test-{}", crate::net::unique_id()));
        std::fs::create_dir(&root).unwrap();
        let occupied = root.join("occupied");
        std::fs::write(&occupied, b"keep").unwrap();
        let fresh = root.join("fresh");
        let mut paths = [occupied.clone(), fresh.clone()].into_iter();
        let (path, mut file) = create_spool(|| Ok(paths.next().unwrap())).unwrap();
        assert_eq!(path, fresh);
        file.write_all(b"upload").unwrap();
        drop(file);
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
        assert_eq!(std::fs::read(&fresh).unwrap(), b"upload");
        assert_eq!(
            create_spool(|| Ok(occupied.clone())).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&occupied, &link).unwrap();
            let mut paths = [link, root.join("after-link")].into_iter();
            let (_, mut file) = create_spool(|| Ok(paths.next().unwrap())).unwrap();
            file.write_all(b"safe").unwrap();
            drop(file);
            assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

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

    /// A collection is addressed with its delimiter, and asking twice
    /// does not add a second one.
    #[test]
    fn collection_paths_end_in_one_slash() {
        assert_eq!(collection("/work"), "/work/");
        assert_eq!(collection("/work/"), "/work/");
        assert_eq!(collection("/"), "/");
        assert_eq!(collection(""), "/");
    }

    /// The redirect predicate: only the collection delimiter on the very
    /// same resource is followed. Everything else keeps the credential
    /// where it was sent (ticket 378).
    #[test]
    fn only_a_trailing_slash_on_the_same_origin_is_followed() {
        let base = "https://dav.test:8443";
        // The redirect Apache mod_dav actually sends, absolute and relative.
        assert_eq!(
            slash_redirect(base, "/work", "https://dav.test:8443/work/"),
            Some("/work/".to_string())
        );
        assert_eq!(
            slash_redirect(base, "/work", "/work/"),
            Some("/work/".to_string())
        );
        assert_eq!(
            slash_redirect(base, "/tree/.hidden-dir", "/tree/.hidden-dir/"),
            Some("/tree/.hidden-dir/".to_string())
        );
        // Another host, a scheme downgrade, another port.
        assert_eq!(
            slash_redirect(base, "/work", "https://evil.test:8443/work/"),
            None
        );
        assert_eq!(
            slash_redirect(base, "/work", "http://dav.test:8443/work/"),
            None
        );
        assert_eq!(
            slash_redirect(base, "/work", "https://dav.test:9443/work/"),
            None
        );
        // Same origin, but not the same resource.
        assert_eq!(slash_redirect(base, "/work", "/other/"), None);
        assert_eq!(slash_redirect(base, "/work", "/work/sub/"), None);
        assert_eq!(slash_redirect(base, "/work", "/work"), None);
        assert_eq!(slash_redirect(base, "/work", "/"), None);
        assert_eq!(slash_redirect(base, "/work", "/work/?login=1"), None);
        assert_eq!(slash_redirect(base, "/work", ""), None);
        // An encoded name keeps its encoding in the retry path.
        assert_eq!(
            slash_redirect(base, "/tree/a%20b", "/tree/a%20b/"),
            Some("/tree/a%20b/".to_string())
        );
    }

    /// A redirect that is not the collection delimiter surfaces as a
    /// protocol error, not as `internal.unexpected`.
    #[test]
    fn an_unresolvable_redirect_is_a_protocol_error() {
        let error = redirect_error("/work", 302, "https://evil.test/work/");
        assert!(error.is(code::NET_PROTOCOL));
        assert_eq!(
            error.params().get("status").map(String::as_str),
            Some("302")
        );
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
