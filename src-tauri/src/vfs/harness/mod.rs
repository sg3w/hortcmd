// ============================================================
// The remote-protocol integration suite (TICKET-038).
//
// These tests talk to real SFTP, FTP, explicit and implicit FTPS, SMB
// and WebDAV servers — the wire behaviour no mock can stand in for. They
// are `#[ignore]`d because they need those servers, and the servers come
// from the harness that also runs them:
//
//   node harness/harness.mjs run
//
// The harness provisions, seeds, runs this suite and tears everything
// down. It passes two environment variables:
//
//   HORTCMD_HARNESS    the run's run.json — ports, credentials, and the
//                      manifest of the tree each server was seeded with
//   HORTCMD_CONFIG_DIR a throwaway configuration directory, so accepting
//                      a host key here never touches the real one
//
// Without `HORTCMD_HARNESS` every test here *skips* rather than fails:
// run by hand with `--ignored` and no harness, they would otherwise be a
// wall of connection errors that says nothing.
//
// The layout mirrors the three questions the suite answers:
//
//   conformance – does every provider implement the same contract?
//   transfers   – does the engine move bytes correctly in all four
//                 directions (local↔remote, remote↔remote)?
//   failure     – what happens when the server, the certificate or the
//                 host key is not what it should be?
// ============================================================

mod conformance;
mod failure;
mod mcp;
mod refresh;
mod transfers;

use super::Provider;
use crate::net::hostkey::{self, ConnectError};
use crate::net::session::Profile;
use serde::Deserialize;
use std::sync::Arc;

// ---------- What the harness tells us ----------

/// One seeded entry, as `harness.mjs` wrote it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SeedEntry {
    /// Relative to the service's root, always with `/` separators.
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

/// One provisioned server.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Service {
    /// The compose service, for stopping it on purpose.
    pub container: String,
    /// `sftp` | `ftp` | `smb` | `webdav` — which provider to build.
    pub protocol: String,
    pub port: u16,
    /// `ssh` | `none` | `explicit` | `implicit` | `https` | `smb`.
    pub tls: String,
    /// The directory the seeded tree sits in, as an address path.
    pub root: String,
    /// Whether the *server* keeps the account inside that tree.
    pub confined: bool,
    pub seed: Vec<SeedEntry>,
}

impl Service {
    /// The root as a *prefix*, for building `{prefix}/tree/readme.txt`.
    /// A root of `"/"` trims to nothing, which is exactly right here.
    pub fn prefix(&self) -> &str {
        self.root.trim_end_matches('/')
    }

    /// The root as a *path*, for handing it to a provider. A root of
    /// `"/"` must stay `"/"`: the empty string is not an address, and
    /// `vfs-location` rejects it with `validation.badInput` before the
    /// provider is ever asked (ticket 379).
    pub fn dir(&self) -> &str {
        let prefix = self.prefix();
        if prefix.is_empty() {
            "/"
        } else {
            prefix
        }
    }
}

/// The whole run.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Harness {
    pub id: String,
    pub project: String,
    pub dir: String,
    pub user: String,
    pub password: String,
    pub ca_cert: String,
    pub services: std::collections::BTreeMap<String, Service>,
}

impl Harness {
    /// The run described by `HORTCMD_HARNESS`, or `None` when the suite
    /// was started without the harness.
    pub fn load() -> Option<Harness> {
        let path = std::env::var_os("HORTCMD_HARNESS")?;
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("HORTCMD_HARNESS={:?} is unreadable: {}", path, e));
        Some(
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|e| panic!("HORTCMD_HARNESS={:?} is not a run.json: {}", path, e)),
        )
    }

    pub fn service(&self, name: &str) -> &Service {
        self.services
            .get(name)
            .unwrap_or_else(|| panic!("the harness has no service {:?}", name))
    }

    /// A profile pointing at one of the run's servers.
    pub fn profile(&self, name: &str) -> Profile {
        let svc = self.service(name);
        // WebDAV needs the scheme spelled out: a non-80 port otherwise
        // implies HTTPS, which is right for a real deployment and wrong
        // for a throwaway HTTP server on a random port.
        let host = match (svc.protocol.as_str(), svc.tls.as_str()) {
            ("webdav", "https") => "https://127.0.0.1".to_string(),
            ("webdav", _) => "http://127.0.0.1".to_string(),
            _ => "127.0.0.1".to_string(),
        };
        Profile {
            monitor: None,
            security: crate::net::security::ProfileSecurity {
                plaintext_acknowledged: true,
                certificate: None,
            },
            id: format!("harness-{}-{}", self.id, name),
            name: format!("harness {}", name),
            protocol: protocol_of(svc),
            host,
            port: svc.port,
            user: self.user.clone(),
            auth: "password".to_string(),
            key_path: None,
            start_path: svc.root.clone(),
        }
    }

    /// The address of the seeded tree on one of the servers.
    pub fn address(&self, name: &str) -> String {
        let profile = self.profile(name);
        let root = self.service(name).prefix();
        format!(
            "{}://{}{}",
            profile.scheme(),
            profile.connection_key(),
            root
        )
    }

    /// Connects, answering the first-contact host-key prompt the way the
    /// connection manager does — a fresh server is always an unknown
    /// host, and refusing to proceed is the *correct* default (TICKET-028).
    pub fn connect(&self, name: &str) -> Arc<dyn Provider> {
        match self.try_connect(name) {
            Ok(p) => p,
            Err(e) => panic!("{}: connect failed: {}", name, e),
        }
    }

    /// The same, for callers that expect failure — or that are waiting
    /// for a restarted service and need to ask again.
    pub fn try_connect(&self, name: &str) -> Result<Arc<dyn Provider>, String> {
        let profile = self.profile(name);
        let secret = self.password.as_str();
        match connect_raw(&profile, secret) {
            Ok(p) => Ok(p),
            Err(ConnectError::HostKey(challenge)) => {
                let token = challenge
                    .token
                    .ok_or_else(|| "host key mismatch".to_string())?;
                hostkey::trust(&token).map_err(|e| e.to_string())?;
                connect_raw(&profile, secret).map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

/// The `Profile::protocol` value for a service.
fn protocol_of(svc: &Service) -> String {
    match (svc.protocol.as_str(), svc.tls.as_str()) {
        ("ftp", "explicit") => "ftps".to_string(),
        ("ftp", "implicit") => "ftps-implicit".to_string(),
        _ => svc.protocol.clone(),
    }
}

/// Dispatch to the provider, without the host-key answer.
fn connect_raw(profile: &Profile, secret: &str) -> Result<Arc<dyn Provider>, ConnectError> {
    match profile.protocol.as_str() {
        "sftp" => super::sftp::connect(profile, Some(secret)),
        "ftp" | "ftps" | "ftps-implicit" => {
            super::ftp::connect(profile, Some(secret)).map_err(ConnectError::from)
        }
        "webdav" => super::webdav::connect(profile, Some(secret)).map_err(ConnectError::from),
        "smb" => super::smb::connect(profile, Some(secret)).map_err(ConnectError::from),
        other => panic!("no provider for protocol {:?}", other),
    }
}

/// Every service the conformance suite runs against, in a fixed order so
/// a failure reads the same way twice.
pub(crate) const ALL_SERVICES: [&str; 7] =
    ["sftp", "ftp", "ftps", "ftpsi", "smb", "webdav", "webdavs"];

/// Runs `body` for every service, reporting which ones failed together
/// rather than stopping at the first.
///
/// Seven servers is enough that "SMB is broken" and "SMB *and* WebDAV
/// are broken" are different diagnoses, and a suite that stops at the
/// first failure cannot tell them apart.
pub(crate) fn for_each_service(h: &Harness, what: &str, body: impl Fn(&Harness, &str)) {
    let mut failed = Vec::new();
    for name in ALL_SERVICES {
        println!("\n── {} · {} ──", name, what);
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(h, name))) {
            Ok(()) => println!("   {} OK", name),
            Err(_) => failed.push(name),
        }
    }
    assert!(failed.is_empty(), "{} failed for: {:?}", what, failed);
}

/// The skip notice, printed once per test that has no harness.
pub(crate) fn skip(test: &str) {
    println!(
        "{}: skipped — no HORTCMD_HARNESS. Run the suite with:\n  node harness/harness.mjs run",
        test
    );
}

/// `Some(harness)`, or `None` after printing why the test did nothing.
macro_rules! harness_or_skip {
    ($name:expr) => {
        match crate::vfs::harness::Harness::load() {
            Some(h) => h,
            None => return crate::vfs::harness::skip($name),
        }
    };
}
pub(crate) use harness_or_skip;
