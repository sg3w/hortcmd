// ============================================================
// Who may talk to the interface (TICKET-041).
//
// Two checks, and both are needed:
//
//   * a **bearer token**, because binding to 127.0.0.1 keeps the
//     interface off the network and does nothing at all about the other
//     programs on this machine — every one of them can reach loopback;
//   * the **Origin and Host headers**, because a web page the user has
//     open is also a local program by that definition. A page cannot set
//     `Authorization` cross-origin without CORS, but it can point a DNS
//     name at 127.0.0.1 and send same-origin requests to it, which is
//     what DNS rebinding is. A `Host` that is not loopback is therefore
//     refused before anything else happens.
//
// The token lives in the OS keychain through the existing
// `CredentialStore`, not in the config file next to the settings: it is
// a credential, and this application already has somewhere to put those.
// ============================================================

use crate::errors::{code, AppError, AppResult};
use crate::net::credentials::{self, CredentialStore};

/// The credential-store id the token is filed under.
const TOKEN_ID: &str = "__mcp_token__";

/// 32 bytes, hex — long enough that guessing is not a strategy, and
/// printable so it can be pasted into a client's configuration.
const TOKEN_BYTES: usize = 32;
static TOKEN_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static TOKEN_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn generation() -> u64 {
    TOKEN_GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn token_loaded() -> bool {
    TOKEN_LOADED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The current token, generating one on first use.
pub fn token() -> AppResult<String> {
    if let Some(existing) = credentials::store().get(TOKEN_ID)? {
        if !existing.is_empty() {
            TOKEN_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
            return Ok(existing);
        }
    }
    regenerate()
}

/// Replaces the token, which invalidates the old one immediately.
pub fn regenerate() -> AppResult<String> {
    let token = random_hex(TOKEN_BYTES)?;
    credentials::store().set(TOKEN_ID, &token)?;
    TOKEN_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    TOKEN_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(token)
}

/// Hex from the operating system's randomness.
///
/// `getrandom` is already in the tree (via the TLS stack); this is the
/// one place in the application that needs unpredictability, and it must
/// not come from a time-seeded PRNG.
pub fn random_hex(bytes: usize) -> AppResult<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| {
        AppError::new(code::INTERNAL_UNEXPECTED)
            .op("mcp-random")
            .detail(e)
    })?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Constant-time comparison.
///
/// A token check that returns early on the first wrong byte tells an
/// attacker how much of a guess was right. The cost of not doing that is
/// a loop that always runs to the end.
pub fn secret_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The token out of an `Authorization` header value, if it carries one.
pub fn bearer_of(header: &str) -> Option<&str> {
    let (scheme, value) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Whether a `Host` or `Origin` header names this machine's loopback.
///
/// Anything else — a hostname that happens to resolve to 127.0.0.1
/// included — is refused. The point is not to identify the caller but to
/// make sure a browser cannot reach the interface with a document's
/// same-origin privileges.
#[cfg(test)]
pub fn is_loopback_authority(value: &str) -> bool {
    let authority = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(value);
    loopback_port(authority).is_some()
}

pub fn loopback_port(value: &str) -> Option<u16> {
    let authority: hyper::http::uri::Authority = value.parse().ok()?;
    if !matches!(authority.host(), "127.0.0.1" | "localhost" | "[::1]") {
        return None;
    }
    match authority.port() {
        Some(port) => port.as_str().parse::<u16>().ok().filter(|p| *p > 0),
        None if value == authority.host() => Some(80),
        None => None,
    }
}

/// The decision for one request, before any tool runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    Allowed,
    /// No token, the wrong token, or a header that is not loopback.
    Refused(&'static str),
}

/// Checks a request's headers against the configured token.
pub fn admit(
    authorization: Option<&str>,
    host: Option<&str>,
    origin: Option<&str>,
    expected: &str,
) -> Admission {
    // Host first: a request that should never have reached us is not
    // worth comparing a token for.
    match host {
        Some(h) if loopback_port(h).is_some() => {}
        Some(_) => return Admission::Refused("host is not loopback"),
        // HTTP/1.1 requires Host; its absence is malformed.
        None => return Admission::Refused("no Host header"),
    }
    // Origin is optional — a command-line client sends none — but a
    // foreign one is a browser trying something.
    if let Some(o) = origin {
        // "null" is what a sandboxed document sends, and is not loopback.
        if o.strip_prefix("http://")
            .and_then(loopback_port)
            .is_none_or(|port| Some(port) != host.and_then(loopback_port))
        {
            return Admission::Refused("origin is not loopback");
        }
    }
    match authorization.and_then(bearer_of) {
        Some(t) if secret_eq(t, expected) => Admission::Allowed,
        Some(_) => Admission::Refused("token does not match"),
        None => Admission::Refused("no bearer token"),
    }
}

/// Turns a refusal into the error the caller sees.
pub fn refusal(reason: &str) -> AppError {
    // Deliberately vague to the client and specific in the log: telling
    // a caller *which* check it failed is telling it how to pass.
    AppError::new(code::SECURITY_BLOCKED_URL)
        .op("mcp-auth")
        .detail(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bearer_header_is_recognised_case_insensitively() {
        assert_eq!(bearer_of("Bearer abc"), Some("abc"));
        assert_eq!(bearer_of("bearer abc"), Some("abc"));
        assert_eq!(bearer_of("BEARER  abc  "), Some("abc"));
        assert_eq!(bearer_of("Basic abc"), None);
        assert_eq!(bearer_of("Bearer"), None);
        assert_eq!(bearer_of("Bearer "), None);
        assert_eq!(bearer_of(""), None);
    }

    #[test]
    fn comparison_does_not_stop_at_the_first_wrong_byte() {
        assert!(secret_eq("abcdef", "abcdef"));
        assert!(!secret_eq("abcdef", "abcdeg"));
        assert!(!secret_eq("abcdef", "abcde"));
        assert!(!secret_eq("", "x"));
        assert!(secret_eq("", ""));
    }

    #[test]
    fn only_loopback_authorities_are_accepted() {
        for good in [
            "127.0.0.1",
            "127.0.0.1:8712",
            "localhost",
            "localhost:8712",
            "http://127.0.0.1:8712",
            "http://localhost:8712",
            "[::1]:8712",
        ] {
            assert!(is_loopback_authority(good), "should accept {}", good);
        }
        for bad in [
            // The rebinding case: a name that resolves to loopback is
            // still not loopback as far as this check is concerned.
            "evil.example.com",
            "http://evil.example.com",
            "http://127.0.0.1.evil.com",
            "https://localhost.evil.com:8712",
            "null",
            "file://",
            "http://127.0.0.1:8712/path",
            "0.0.0.0:8712",
            "192.168.1.10:8712",
            "localhost:garbage",
            "localhost:99999",
            "localhost:",
            "localhost:0",
            "localhost:8712?query",
            "localhost:8712#fragment",
        ] {
            assert!(!is_loopback_authority(bad), "should refuse {}", bad);
        }
    }

    #[test]
    fn admission_needs_the_token_and_a_loopback_host() {
        let ok = |a, h, o| admit(a, h, o, "secret");

        assert_eq!(
            ok(Some("Bearer secret"), Some("127.0.0.1:8712"), None),
            Admission::Allowed
        );
        assert_eq!(
            ok(
                Some("Bearer secret"),
                Some("127.0.0.1:8712"),
                Some("http://localhost:8712")
            ),
            Admission::Allowed
        );

        // Every way it can fail, and each distinguishable in the log.
        assert!(matches!(
            ok(None, Some("127.0.0.1"), None),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ok(Some("Bearer wrong"), Some("127.0.0.1"), None),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ok(Some("Bearer secret"), None, None),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ok(Some("Bearer secret"), Some("evil.example.com"), None),
            Admission::Refused(_)
        ));
        assert!(matches!(
            ok(
                Some("Bearer secret"),
                Some("127.0.0.1"),
                Some("https://evil.example.com")
            ),
            Admission::Refused(_)
        ));
    }

    /// The rebinding attack in one case: the browser sends its own
    /// document's Origin, and the Host it dialled is the attacker's name.
    #[test]
    fn a_rebound_dns_name_is_refused_even_with_the_right_token() {
        assert!(matches!(
            admit(
                Some("Bearer secret"),
                Some("rebind.evil.com:8712"),
                Some("http://rebind.evil.com:8712"),
                "secret",
            ),
            Admission::Refused(_)
        ));
    }

    #[test]
    fn a_generated_token_is_long_random_hex() {
        let a = random_hex(TOKEN_BYTES).unwrap();
        let b = random_hex(TOKEN_BYTES).unwrap();
        assert_eq!(a.len(), TOKEN_BYTES * 2);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two tokens must not be the same");
    }
}
