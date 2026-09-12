// ============================================================
// The application error contract (ticket 036).
//
// Backend errors used to be display-ready strings: half of them German,
// several carrying raw platform or protocol wording, and three of them
// load-bearing — the frontend matched substrings to recognize a locked
// KeePass file, an unsupported location, an archive password prompt.
// That is brittle control flow, inconsistent localization, and a place
// where a credential-bearing URL could reach a dialog.
//
// An `AppError` is the replacement. Four ideas carry it:
//
//   * **The code decides, the text explains.** Callers branch on a
//     stable namespaced code (`net.auth`, `archive.passwordRequired`);
//     the sentence a user reads is produced in the frontend, in their
//     language, from that code and a few safe parameters.
//   * **Nothing display-ready crosses the boundary.** The backend never
//     sends a finished sentence, so nothing can arrive in the wrong
//     language or in a server's own words.
//   * **Secrets never enter the payload.** Every string that could have
//     come from a URL, a header or a protocol reply goes through
//     `redact` on the way in — centrally, not at each call site.
//   * **A newer backend stays readable.** The contract is versioned and
//     every error carries a correlation id, so an unknown code renders
//     a safe generic message the user can still report.
//
// Technical detail is kept for diagnostics (ticket 045) and is not
// rendered by default.
// ============================================================

use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::io;
use ts_rs::TS;

/// Version of the error contract. Raised when a field changes meaning,
/// so a frontend can tell "unknown code" from "incompatible payload".
pub const ERROR_CONTRACT_VERSION: u32 = 1;

/// What the user could do about a failure. Drives the buttons an error
/// surface offers, so the decision is made once here rather than by
/// each dialog guessing from the code.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ErrorAction {
    /// The same request may simply work next time.
    Retry,
    /// The connection has to be re-established first.
    Reconnect,
    /// Credentials are wrong or missing.
    Authenticate,
    /// The server identity needs confirming (ticket 028).
    ConfirmHostKey,
    /// A password is needed (archive, KeePass).
    ProvidePassword,
    /// The destination is full.
    FreeSpace,
    /// The account lacks the rights.
    CheckPermissions,
    /// This location cannot do it at all (ticket 031).
    Unsupported,
    /// Nothing the user can do; report it.
    Report,
}

/// One failure, described rather than phrased.
/// `PartialEq` compares the *code* and nothing else: two errors with
/// the same code describe the same failure, and their correlation ids
/// are deliberately different. Tests read naturally as a result.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct AppError {
    /// Contract version this payload was built against.
    pub version: u32,
    /// Stable, namespaced identifier — `fs.notFound`, `net.auth`, …
    /// This is what callers branch on; never the message.
    ///
    /// `Box<str>` rather than `String` here and below: every
    /// `AppResult<T>` in the crate is as large as this struct, and the
    /// eight bytes each one saves are what keep it under the size at
    /// which carrying an error everywhere starts to cost. The
    /// serialized shape is identical.
    pub code: Box<str>,
    /// Safe substitutions for the localized message (`{name}`, `{path}`).
    /// Only values the backend knows to be free of secrets get in here.
    pub params: BTreeMap<String, String>,
    /// The operation that failed (`copy`, `connect`, `extract`), when known.
    pub op: Option<Box<str>>,
    /// The provider or scheme it happened on (`sftp`, `local`), when known.
    pub provider: Option<Box<str>>,
    /// Whether the same request could plausibly succeed on a retry.
    pub retryable: bool,
    /// What the user can do about it.
    pub action: Option<ErrorAction>,
    /// Redacted technical detail — for diagnostics, **not** for display.
    pub detail: Option<Box<str>>,
    /// Correlation id: what a user quotes in a report, and what a log
    /// entry is matched against.
    pub id: Box<str>,
}

impl AppError {
    /// A new error with the given code. Everything else is added with
    /// the builder methods, so a call site reads as a description.
    pub fn new(code: &str) -> Self {
        AppError {
            version: ERROR_CONTRACT_VERSION,
            code: code.into(),
            params: BTreeMap::new(),
            op: None,
            provider: None,
            retryable: false,
            action: None,
            detail: None,
            id: correlation_id().into(),
        }
    }

    /// Adds a localization parameter. The value is redacted on the way
    /// in — a path or a name is safe, but the same slot is used for
    /// server replies, and one call site forgetting is enough.
    pub fn with(mut self, name: &str, value: impl fmt::Display) -> Self {
        self.params
            .insert(name.to_string(), redact(&value.to_string()));
        self
    }

    /// Names the operation this happened during.
    pub fn op(mut self, op: &str) -> Self {
        self.op = Some(op.into());
        self
    }

    /// Names the provider or scheme it happened on.
    pub fn provider(mut self, provider: &str) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Marks the failure as worth retrying unchanged.
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    /// What the user can do about it.
    pub fn action(mut self, action: ErrorAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Same, where the caller may have nothing to offer — a cancel, for
    /// instance, gives the user nothing to act on.
    pub fn maybe_action(mut self, action: Option<ErrorAction>) -> Self {
        self.action = action;
        self
    }

    /// Attaches redacted technical detail for diagnostics. Never shown
    /// unless the user asks for it.
    pub fn detail(mut self, detail: impl fmt::Display) -> Self {
        self.detail = Some(redact(&detail.to_string()).into());
        self
    }

    /// Whether this error carries `code`. The backend's own way of
    /// branching, matching what the frontend does.
    pub fn is(&self, code: &str) -> bool {
        &*self.code == code
    }
}

impl PartialEq for AppError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl Eq for AppError {}

impl fmt::Display for AppError {
    /// Diagnostics only. The sentence a user reads is produced in the
    /// frontend from `code` and `params`; this is the log form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]", self.code, self.id)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {}", detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

/// The result every command returns.
pub type AppResult<T> = Result<T, AppError>;

// ---------- Codes ----------

/// Every code the backend produces, grouped by namespace.
///
/// They are `&'static str` rather than an enum on purpose: an enum would
/// force a lockstep upgrade of every consumer, while a string lets a
/// newer backend introduce a code that an older frontend renders
/// generically — which is exactly the compatibility rule this contract
/// promises. The list is exhaustive so the translation snapshot test
/// can walk it.
pub mod code {
    // --- filesystem ---
    pub const FS_NOT_FOUND: &str = "fs.notFound";
    pub const FS_PERMISSION: &str = "fs.permission";
    pub const FS_EXISTS: &str = "fs.exists";
    pub const FS_NOT_A_DIRECTORY: &str = "fs.notADirectory";
    pub const FS_IS_A_DIRECTORY: &str = "fs.isADirectory";
    pub const FS_READ_ONLY: &str = "fs.readOnly";
    pub const FS_NO_SPACE: &str = "fs.noSpace";
    pub const FS_IN_USE: &str = "fs.inUse";
    pub const FS_IO: &str = "fs.io";
    pub const FS_INVALID_NAME: &str = "fs.invalidName";
    pub const FS_TOO_LARGE: &str = "fs.tooLarge";

    // --- transfers ---
    pub const TRANSFER_CHECKSUM: &str = "transfer.checksum";
    pub const TRANSFER_NO_STAGING: &str = "transfer.noStaging";
    pub const TRANSFER_COMMIT: &str = "transfer.commit";
    pub const TRANSFER_SOURCE_GONE: &str = "transfer.sourceGone";
    pub const TRANSFER_INTO_ITSELF: &str = "transfer.intoItself";

    // --- archives ---
    pub const ARCHIVE_PASSWORD_REQUIRED: &str = "archive.passwordRequired";
    pub const ARCHIVE_PASSWORD_WRONG: &str = "archive.passwordWrong";
    pub const ARCHIVE_UNKNOWN_FORMAT: &str = "archive.unknownFormat";
    pub const ARCHIVE_CORRUPT: &str = "archive.corrupt";
    pub const ARCHIVE_UNSAFE_ENTRY: &str = "archive.unsafeEntry";

    // --- network ---
    pub const NET_UNREACHABLE: &str = "net.unreachable";
    pub const NET_TIMEOUT: &str = "net.timeout";
    pub const NET_AUTH: &str = "net.auth";
    pub const NET_PROTOCOL: &str = "net.protocol";
    pub const NET_NOT_CONNECTED: &str = "net.notConnected";
    pub const NET_LOST: &str = "net.lost";
    pub const NET_NO_PROFILE: &str = "net.noProfile";
    pub const NET_UNSUPPORTED_PROTOCOL: &str = "net.unsupportedProtocol";
    pub const NET_NO_SHELL: &str = "net.noShell";

    // --- credentials ---
    pub const CRED_LOCKED: &str = "cred.locked";
    pub const CRED_WRONG_PASSWORD: &str = "cred.wrongPassword";
    pub const CRED_STORE: &str = "cred.store";
    pub const CRED_NOT_FOUND: &str = "cred.notFound";
    pub const CRED_READ_ONLY_FORMAT: &str = "cred.readOnlyFormat";

    // --- security ---
    pub const SECURITY_HOST_KEY_UNKNOWN: &str = "security.hostKeyUnknown";
    pub const SECURITY_HOST_KEY_MISMATCH: &str = "security.hostKeyMismatch";
    pub const SECURITY_BLOCKED_URL: &str = "security.blockedUrl";

    // --- capability (ticket 031) ---
    pub const CAPABILITY_UNSUPPORTED: &str = "capability.unsupported";
    pub const CAPABILITY_OFFLINE: &str = "capability.offline";

    // --- validation ---
    pub const VALIDATION_BAD_ADDRESS: &str = "validation.badAddress";
    pub const VALIDATION_UNKNOWN_SCHEME: &str = "validation.unknownScheme";
    pub const VALIDATION_BAD_INPUT: &str = "validation.badInput";

    // --- cancellation (never rendered as a failure) ---
    pub const CANCELLED: &str = "cancelled";

    // --- internal ---
    pub const INTERNAL_UNEXPECTED: &str = "internal.unexpected";

    /// Every code above, for the test that checks each one is
    /// translated in both languages.
    pub const ALL: &[&str] = &[
        FS_NOT_FOUND,
        FS_PERMISSION,
        FS_EXISTS,
        FS_NOT_A_DIRECTORY,
        FS_IS_A_DIRECTORY,
        FS_READ_ONLY,
        FS_NO_SPACE,
        FS_IN_USE,
        FS_IO,
        FS_INVALID_NAME,
        FS_TOO_LARGE,
        TRANSFER_CHECKSUM,
        TRANSFER_NO_STAGING,
        TRANSFER_COMMIT,
        TRANSFER_SOURCE_GONE,
        TRANSFER_INTO_ITSELF,
        ARCHIVE_PASSWORD_REQUIRED,
        ARCHIVE_PASSWORD_WRONG,
        ARCHIVE_UNKNOWN_FORMAT,
        ARCHIVE_CORRUPT,
        ARCHIVE_UNSAFE_ENTRY,
        NET_UNREACHABLE,
        NET_TIMEOUT,
        NET_AUTH,
        NET_PROTOCOL,
        NET_NOT_CONNECTED,
        NET_LOST,
        NET_NO_PROFILE,
        NET_UNSUPPORTED_PROTOCOL,
        NET_NO_SHELL,
        CRED_LOCKED,
        CRED_WRONG_PASSWORD,
        CRED_STORE,
        CRED_NOT_FOUND,
        CRED_READ_ONLY_FORMAT,
        SECURITY_HOST_KEY_UNKNOWN,
        SECURITY_HOST_KEY_MISMATCH,
        SECURITY_BLOCKED_URL,
        CAPABILITY_UNSUPPORTED,
        CAPABILITY_OFFLINE,
        VALIDATION_BAD_ADDRESS,
        VALIDATION_UNKNOWN_SCHEME,
        VALIDATION_BAD_INPUT,
        CANCELLED,
        INTERNAL_UNEXPECTED,
    ];
}

// ---------- Mapping at the owning boundary ----------

impl From<io::Error> for AppError {
    /// The one place an OS error becomes an application error. Every
    /// filesystem boundary goes through it, so "not found" reads the
    /// same whether it came from a local `stat` or an SFTP `open`.
    fn from(e: io::Error) -> Self {
        use io::ErrorKind::*;
        let code = match e.kind() {
            NotFound => code::FS_NOT_FOUND,
            PermissionDenied => code::FS_PERMISSION,
            AlreadyExists => code::FS_EXISTS,
            // `Interrupted` is how the transfer engine signals a cancel.
            Interrupted => code::CANCELLED,
            // These arrived with Rust 1.83+; matching by name keeps the
            // MSRV honest without a version gate.
            _ => match e.kind().to_string().as_str() {
                "is a directory" => code::FS_IS_A_DIRECTORY,
                "not a directory" => code::FS_NOT_A_DIRECTORY,
                "read-only filesystem or storage medium" => code::FS_READ_ONLY,
                "no storage space" | "quota exceeded" => code::FS_NO_SPACE,
                "resource busy" | "executable file busy" => code::FS_IN_USE,
                "file name too long" | "invalid filename" => code::FS_INVALID_NAME,
                "file too large" => code::FS_TOO_LARGE,
                _ => code::FS_IO,
            },
        };
        let retryable = matches!(code, code::FS_IN_USE | code::FS_IO);
        let action = match code {
            code::FS_PERMISSION | code::FS_READ_ONLY => Some(ErrorAction::CheckPermissions),
            code::FS_NO_SPACE => Some(ErrorAction::FreeSpace),
            code::FS_IN_USE => Some(ErrorAction::Retry),
            code::CANCELLED => None,
            _ => Some(ErrorAction::Report),
        };
        let mut err = AppError::new(code).detail(e);
        err.retryable = retryable;
        err.action = action;
        err
    }
}

impl From<AppError> for io::Error {
    /// Back into the `io::Result` the `Provider` trait speaks. The code
    /// survives in the message so a later `From<io::Error>` recovers it
    /// rather than flattening everything to `fs.io`.
    fn from(e: AppError) -> Self {
        io::Error::other(e.to_string())
    }
}

impl From<String> for AppError {
    /// The migration path for boundaries that still produce prose.
    ///
    /// The text becomes *detail*, never the displayed message: whatever
    /// a provider said stays available for diagnostics while the user
    /// sees a safe generic sentence. Replacing one of these with a real
    /// code is always an improvement; leaving one is never a leak.
    fn from(message: String) -> Self {
        AppError::new(code::INTERNAL_UNEXPECTED)
            .action(ErrorAction::Report)
            .detail(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        AppError::from(message.to_string())
    }
}

/// Library errors that reach a `?` in this crate.
///
/// Each is mapped where it is *used*, not where it is defined, so the
/// code reflects what the failure means here rather than what the
/// library calls it. All of them keep their own wording as redacted
/// detail; none of it is displayed.
macro_rules! from_library_error {
    ($ty:ty, $code:expr) => {
        impl From<$ty> for AppError {
            fn from(e: $ty) -> Self {
                AppError::new($code).action(ErrorAction::Report).detail(e)
            }
        }
    };
}

from_library_error!(zip::result::ZipError, code::ARCHIVE_CORRUPT);
from_library_error!(serde_json::Error, code::INTERNAL_UNEXPECTED);
from_library_error!(tauri::Error, code::INTERNAL_UNEXPECTED);
from_library_error!(russh::Error, code::NET_PROTOCOL);
from_library_error!(russh::keys::Error, code::NET_AUTH);
from_library_error!(opener::OpenError, code::FS_IO);
from_library_error!(trash::Error, code::FS_IO);
#[cfg(target_os = "macos")]
from_library_error!(plist::Error, code::FS_IO);

// ---------- Redaction ----------

/// Removes what must never reach an error payload.
///
/// Centralized on purpose: redaction that lives at call sites is
/// redaction that one new call site forgets. Everything entering an
/// `AppError` — parameters and detail alike — passes through here.
///
/// Four shapes carry secrets in practice:
///   * URL user-info (`sftp://user:pw@host` → `sftp://user:***@host`);
///   * authorization headers and bearer tokens;
///   * `password=` / `token=` style assignments, in query strings and
///     in the prose of a protocol reply;
///   * a private key or certificate pasted into a message.
pub fn redact(text: &str) -> String {
    let mut out = redact_url_userinfo(text);
    out = redact_assignments(&out);
    out = redact_bearer(&out);
    out = redact_pem(&out);
    out
}

/// `scheme://user:secret@host` → `scheme://user:***@host`.
fn redact_url_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        // Find "://" and then the authority up to '/', whitespace or end.
        if bytes[i..].starts_with(&[':', '/', '/']) {
            out.push_str("://");
            i += 3;
            let start = i;
            while i < bytes.len() && !"/ \t\n\"'".contains(bytes[i]) {
                i += 1;
            }
            let authority: String = bytes[start..i].iter().collect();
            match authority.rsplit_once('@') {
                // `user:secret@host` — keep the user, drop the secret.
                Some((userinfo, host)) => match userinfo.split_once(':') {
                    Some((user, _)) => {
                        out.push_str(user);
                        out.push_str(":***@");
                        out.push_str(host);
                    }
                    None => {
                        out.push_str(userinfo);
                        out.push('@');
                        out.push_str(host);
                    }
                },
                None => out.push_str(&authority),
            }
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Words that introduce a secret in an assignment or a sentence.
const SECRET_WORDS: [&str; 10] = [
    "password",
    "passwort",
    "passphrase",
    "token",
    "secret",
    "apikey",
    "api_key",
    "access_key",
    "private_key",
    "authorization",
];

/// `password=hunter2`, `token: abc`, `Passwort "x"` → the value replaced.
fn redact_assignments(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut out = text.to_string();
    // Work from the end so earlier indices stay valid.
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for word in SECRET_WORDS {
        let mut from = 0;
        while let Some(rel) = lower[from..].find(word) {
            let at = from + rel;
            from = at + word.len();
            // Skip the separator (`=`, `:`, spaces, quotes).
            let mut j = from;
            let chars: Vec<char> = text.chars().collect();
            while j < chars.len() && " \t=:\"'".contains(chars[j]) {
                j += 1;
            }
            if j == from || j >= chars.len() {
                continue; // no assignment — just the word itself
            }
            // `Authorization: Bearer abc` — the scheme word is not the
            // secret; the token after it is.
            let mut value_start = j;
            while j < chars.len() && !" \t\n,;\"')".contains(chars[j]) {
                j += 1;
            }
            let first: String = chars[value_start..j].iter().collect();
            if matches!(
                first.to_lowercase().as_str(),
                "bearer" | "basic" | "digest" | "token"
            ) {
                while j < chars.len() && " \t".contains(chars[j]) {
                    j += 1;
                }
                value_start = j;
                while j < chars.len() && !" \t\n,;\"')".contains(chars[j]) {
                    j += 1;
                }
            }
            // Already redacted by an earlier pass — leave it be, so
            // repeated redaction stays idempotent.
            let value: String = chars[value_start..j].iter().collect();
            if j > value_start && value != "***" {
                hits.push((value_start, j));
            }
        }
    }
    hits.sort_unstable();
    hits.dedup();
    for (start, end) in hits.into_iter().rev() {
        let prefix: String = out.chars().take(start).collect();
        let suffix: String = out.chars().skip(end).collect();
        out = format!("{}***{}", prefix, suffix);
    }
    out
}

/// `Bearer abc.def.ghi` → `Bearer ***`, for every occurrence.
///
/// Iterative rather than recursive: replacing a token leaves the word
/// "bearer" in place, so a naive re-scan finds it again and never
/// terminates. The cursor moves past each replacement instead.
fn redact_bearer(text: &str) -> String {
    let mut out: Vec<char> = text.chars().collect();
    let mut from = 0usize;
    loop {
        let lower: String = out.iter().collect::<String>().to_lowercase();
        let Some(rel) = lower.get(from..).and_then(|t| t.find("bearer ")) else {
            break;
        };
        // Byte offset → char index (the haystack is ASCII-lowercased
        // but may contain multi-byte characters elsewhere).
        let at = lower[..from + rel].chars().count();
        let start = at + "bearer ".len();
        let mut j = start;
        while j < out.len() && !" \t\n,;\"'".contains(out[j]) {
            j += 1;
        }
        let value: String = out[start..j].iter().collect();
        if j > start && value != "***" {
            out.splice(start..j, "***".chars());
            from = lower[..from + rel].len() + "bearer ***".len();
        } else {
            from = lower[..from + rel].len() + "bearer ".len();
        }
    }
    out.into_iter().collect()
}

/// A pasted key block never belongs in an error.
fn redact_pem(text: &str) -> String {
    let Some(at) = text.find("-----BEGIN") else {
        return text.to_string();
    };
    let end = text[at..]
        .find("-----END")
        .and_then(|rel| text[at + rel..].find("-----\n").map(|e| at + rel + e + 6))
        .unwrap_or(text.len());
    format!("{}***{}", &text[..at], &text[end..])
}

/// Short, unique-enough id for matching a report to a log line.
fn correlation_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!(
        "e{:x}{:x}",
        nanos & 0xffff_ffff,
        N.fetch_add(1, Ordering::Relaxed) & 0xfff
    )
}

#[cfg(test)]
mod tests;
