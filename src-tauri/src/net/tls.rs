// ============================================================
// An extra trust anchor for private infrastructure.
//
// hortcmd validates TLS certificates with the platform's own trust store
// and offers no "continue anyway" — a certificate that does not verify
// fails the connection (README § Security). That is the right default and
// stays the default.
//
// It also leaves out a case that is ordinary rather than careless: a
// self-hosted FTPS or WebDAV server whose certificate is issued by the
// operator's own CA. The established answer is an *additional* trust
// anchor the user chooses, not a weakened check — `CURL_CA_BUNDLE`,
// `NODE_EXTRA_CA_CERTS` and `GIT_SSL_CAINFO` all work this way. So does
// this:
//
//   HORTCMD_EXTRA_CA=/path/to/ca.pem
//
// Every certificate in that PEM file is added to the roots for FTPS and
// WebDAV. Validation is otherwise untouched: hostname, expiry and chain
// are still checked, and an unknown issuer still fails. The protocol
// harness (TICKET-038) uses it to reach its own throwaway TLS servers
// without installing anything into the machine's trust store.
// ============================================================

use crate::errors::{code, AppError, AppResult};
use std::path::PathBuf;

const EXTRA_CA: &str = "HORTCMD_EXTRA_CA";

/// The configured bundle, if the variable names one.
pub fn extra_ca_path() -> Option<PathBuf> {
    resolve_path(std::env::var_os(EXTRA_CA))
}

/// The decision itself, with the environment passed in — see
/// [`crate::config`] for why tests do not touch the real variable.
fn resolve_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// The PEM certificates in the configured bundle.
///
/// A configured but unreadable or empty bundle is an error rather than a
/// silent fallback: the user asked for those anchors, and quietly
/// continuing without them would connect under a trust assumption they
/// did not make.
pub fn extra_ca_pems() -> AppResult<Vec<Vec<u8>>> {
    read_bundle(extra_ca_path())
}

fn read_bundle(path: Option<PathBuf>) -> AppResult<Vec<Vec<u8>>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = std::fs::read(&path).map_err(|e| {
        AppError::new(code::NET_TLS)
            .op("extra-ca")
            .detail(format!("{}: {}", path.display(), e))
    })?;
    let blocks = split_pem(&bytes);
    if blocks.is_empty() {
        return Err(AppError::new(code::NET_TLS)
            .op("extra-ca")
            .detail(format!("{} enthält kein PEM-Zertifikat", path.display())));
    }
    Ok(blocks)
}

/// Splits a bundle into individual `-----BEGIN CERTIFICATE-----` blocks.
///
/// Both back-ends want one certificate at a time, and a bundle with an
/// operator's root *and* intermediate is the normal shape.
fn split_pem(bytes: &[u8]) -> Vec<Vec<u8>> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut rest = text.as_ref();
    while let Some(start) = rest.find(BEGIN) {
        let Some(end) = rest[start..].find(END) else {
            break;
        };
        let stop = start + end + END.len();
        out.push(rest.as_bytes()[start..stop].to_vec());
        rest = &rest[stop..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_bundle_into_its_certificates() {
        let pem = b"noise before\n\
-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n\
between\n\
-----BEGIN CERTIFICATE-----\nBBBB\n-----END CERTIFICATE-----\n\
trailing";
        let blocks = split_pem(pem);
        assert_eq!(blocks.len(), 2, "both certificates must be found");
        assert!(String::from_utf8_lossy(&blocks[0]).contains("AAAA"));
        assert!(String::from_utf8_lossy(&blocks[1]).contains("BBBB"));
        // A block must be complete on both ends, or the TLS stack will
        // reject it with a message nobody can act on.
        for b in &blocks {
            let s = String::from_utf8_lossy(b);
            assert!(s.starts_with("-----BEGIN CERTIFICATE-----"));
            assert!(s.ends_with("-----END CERTIFICATE-----"));
        }
    }

    #[test]
    fn an_unterminated_block_is_not_returned() {
        let pem = b"-----BEGIN CERTIFICATE-----\nAAAA\n";
        assert!(
            split_pem(pem).is_empty(),
            "half a certificate is no certificate"
        );
    }

    #[test]
    fn no_variable_means_no_extra_roots() {
        assert!(resolve_path(None).is_none());
        assert!(
            resolve_path(Some(std::ffi::OsString::new())).is_none(),
            "an empty value is no value"
        );
        assert!(
            read_bundle(None)
                .expect("no bundle is not an error")
                .is_empty(),
            "without a bundle the platform roots stand alone"
        );
    }

    /// A bundle that was named but cannot be read must fail loudly: the
    /// alternative is connecting with fewer anchors than the user asked
    /// for and never saying so.
    #[test]
    fn a_named_but_missing_bundle_is_an_error() {
        let err = read_bundle(Some(PathBuf::from("/definitely/not/here.pem")))
            .expect_err("a missing bundle must not pass silently");
        assert!(err.is(code::NET_TLS), "unexpected code: {:?}", err);
    }

    /// A file that exists but holds no certificate is the same mistake
    /// with a different shape — a path typo pointing at a key, say.
    #[test]
    fn a_bundle_without_certificates_is_an_error() {
        let path = std::env::temp_dir().join("hortcmd-empty-bundle.pem");
        std::fs::write(&path, b"not a certificate\n").unwrap();
        let err = read_bundle(Some(path.clone())).expect_err("empty bundle");
        assert!(err.is(code::NET_TLS));
        let _ = std::fs::remove_file(path);
    }
}
