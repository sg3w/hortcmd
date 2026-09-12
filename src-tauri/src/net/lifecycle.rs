// ============================================================
// Lifecycle of a remote connection (ticket 032).
//
// Connecting is the one operation in hortcmd that talks to a machine
// nobody controls. It can hang, it can be answered by a socket that
// never completes a handshake, and the user can change their mind in
// the middle of it. Before this module, `net_connect` did keychain
// access plus protocol setup in one synchronous command body: the UI
// awaited it with no pending state, offered the connect button again
// during the attempt, and had no way to stop one.
//
// This module is the state machine behind that. Three ideas carry it:
//
//   * **One attempt per profile.** Each attempt gets an id and a
//     monotonically increasing generation. A second connect click finds
//     the running attempt instead of opening a second socket.
//   * **Bounded and cancellable.** A 15-second transport timeout and a
//     45-second overall timeout, plus an explicit cancel. The caller
//     never waits longer than the overall cap, whatever the server does.
//   * **Only a finished attempt may win.** The provider enters the live
//     registry in `settle`, under one lock, and only if its generation
//     is still current. A cancelled or superseded result is dropped —
//     which closes the connection it opened — and a failed reconnect
//     leaves the previously working session exactly where it was.
//
// The registry lock is never held across keychain access or network
// I/O: every phase runs on a worker thread and reaches back in here
// only to record a decision.
// ============================================================

use crate::errors::{code, AppError, ErrorAction};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use ts_rs::TS;

/// How long the bare transport (DNS + TCP) may take. Reaching the port
/// is the one step every protocol shares, so it is timed uniformly here
/// rather than per client — most of which cannot express it at all.
pub const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(15);

/// Cap on the whole attempt: transport, handshake, authentication, host
/// verification and the provider's own readiness probe. Deliberately a
/// constant, not a setting (ticket 032 leaves that to a later ticket).
pub const OVERALL_TIMEOUT: Duration = Duration::from_secs(45);

/// Where a profile stands right now. The frontend renders from this and
/// never infers a connection state from an error message.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Phase {
    /// No session and no attempt.
    Disconnected,
    /// An attempt is running; `attempt` names it.
    Connecting,
    /// A live provider is in the registry.
    Connected,
    /// Teardown is running.
    Disconnecting,
    /// The last attempt failed; `error` says how.
    Failed,
}

/// Why an attempt did not end in a connection. Separate from the
/// message so the UI can react to the *kind* — a timeout offers a
/// retry, a host-key failure opens the confirmation, a keychain error
/// points at the credential store — instead of matching on prose.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum FailureKind {
    /// A phase exceeded its time limit.
    Timeout,
    /// The user (or a superseding attempt) stopped this one.
    Cancelled,
    /// The server refused the credentials.
    Auth,
    /// The server identity is unknown or has changed (ticket 028).
    HostKey,
    /// DNS or TCP did not get us to the port.
    Unreachable,
    /// The endpoint answered but the protocol exchange failed.
    Protocol,
    /// The secret could not be read from the credential store.
    Keychain,
}

impl FailureKind {
    /// The error code this kind corresponds to. One mapping, so the
    /// lifecycle and the error contract cannot drift apart.
    pub fn code(self) -> &'static str {
        match self {
            FailureKind::Timeout => code::NET_TIMEOUT,
            FailureKind::Cancelled => code::CANCELLED,
            FailureKind::Auth => code::NET_AUTH,
            FailureKind::HostKey => code::SECURITY_HOST_KEY_UNKNOWN,
            FailureKind::Unreachable => code::NET_UNREACHABLE,
            FailureKind::Protocol => code::NET_PROTOCOL,
            FailureKind::Keychain => code::CRED_STORE,
        }
    }

    /// What the user can do about a failure of this kind.
    pub fn action(self) -> Option<ErrorAction> {
        match self {
            FailureKind::Timeout | FailureKind::Unreachable => Some(ErrorAction::Retry),
            FailureKind::Cancelled => None,
            FailureKind::Auth => Some(ErrorAction::Authenticate),
            FailureKind::HostKey => Some(ErrorAction::ConfirmHostKey),
            FailureKind::Protocol => Some(ErrorAction::Report),
            FailureKind::Keychain => Some(ErrorAction::ProvidePassword),
        }
    }
}

/// A typed attempt failure: the kind drives behavior, the message is
/// what the user reads.
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ConnFailure {
    pub kind: FailureKind,
    /// The typed failure behind it (ticket 036). The lifecycle's own
    /// `kind` drives the dialog; this is what the user is told.
    pub error: AppError,
}

impl ConnFailure {
    /// A failure described by its lifecycle kind alone. The code is
    /// derived from the kind, so the two can never disagree.
    pub fn new(kind: FailureKind, detail: impl std::fmt::Display) -> Self {
        ConnFailure {
            kind,
            error: AppError::new(kind.code())
                .op("connect")
                .maybe_action(kind.action())
                .detail(detail),
        }
    }

    /// A failure that already carries a typed error from further down.
    pub fn from_error(kind: FailureKind, error: AppError) -> Self {
        ConnFailure { kind, error }
    }
}

/// A running connect request. The handle the worker thread carries and
/// the token `net_cancel_connect` addresses.
#[derive(Clone)]
pub struct Attempt {
    pub id: String,
    pub profile_id: String,
    pub generation: u64,
    cancel: Arc<AtomicBool>,
}

impl Attempt {
    /// Whether this attempt has been cancelled or superseded. Checked
    /// between phases, so a cancel during a long handshake takes effect
    /// at the next boundary at the latest — and the finished result is
    /// discarded in [`settle`] regardless.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// `Err` with the cancellation failure when this attempt is no
    /// longer wanted — the shape every phase check uses.
    pub fn check(&self) -> Result<(), ConnFailure> {
        if self.cancelled() {
            return Err(ConnFailure::new(
                FailureKind::Cancelled,
                "Verbindungsaufbau abgebrochen",
            ));
        }
        Ok(())
    }
}

/// One profile's lifecycle record.
#[derive(Clone)]
struct Entry {
    phase: Phase,
    attempt: Option<Attempt>,
    error: Option<ConnFailure>,
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            phase: Phase::Disconnected,
            attempt: None,
            error: None,
        }
    }
}

fn table() -> &'static Mutex<HashMap<String, Entry>> {
    static TABLE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_generation() -> u64 {
    static GEN: AtomicU64 = AtomicU64::new(1);
    GEN.fetch_add(1, Ordering::SeqCst)
}

/// What [`begin`] decided.
pub enum Start {
    /// A fresh attempt was registered; the caller owns it.
    Started(Attempt),
    /// One was already running for this profile — the caller shows that
    /// one instead of opening a second socket.
    AlreadyRunning(Attempt),
}

/// Registers an attempt for `profile_id`, unless one is already running.
pub fn begin(profile_id: &str) -> Start {
    let mut map = table().lock().unwrap();
    let entry = map.entry(profile_id.to_string()).or_default();
    if entry.phase == Phase::Connecting {
        if let Some(running) = entry.attempt.clone() {
            return Start::AlreadyRunning(running);
        }
    }
    let attempt = Attempt {
        id: format!("conn-{}", super::unique_id()),
        profile_id: profile_id.to_string(),
        generation: next_generation(),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    *entry = Entry {
        phase: Phase::Connecting,
        attempt: Some(attempt.clone()),
        error: None,
    };
    Start::Started(attempt)
}

/// Cancels the attempt with this id. Idempotent: an unknown, finished or
/// already-cancelled id is a no-op, and cancelling never itself fails.
/// Returns the profile whose attempt was stopped, if any.
pub fn cancel(attempt_id: &str) -> Option<String> {
    let mut map = table().lock().unwrap();
    let hit = map
        .iter_mut()
        .find(|(_, e)| e.attempt.as_ref().is_some_and(|a| a.id == attempt_id));
    let (profile_id, entry) = hit?;
    let profile_id = profile_id.clone();
    if let Some(attempt) = &entry.attempt {
        attempt.cancel.store(true, Ordering::SeqCst);
    }
    // The phase only moves once the worker settles: the attempt may be
    // deep in a handshake, and claiming "disconnected" before its
    // sockets are gone would be a lie. `settle` turns the cancelled
    // flag into the final state.
    Some(profile_id)
}

/// Cancels whatever is running for `profile_id` (disconnect, delete).
pub fn cancel_for_profile(profile_id: &str) -> Option<String> {
    let map = table().lock().unwrap();
    let id = map.get(profile_id)?.attempt.as_ref()?.id.clone();
    drop(map);
    cancel(&id);
    Some(id)
}

/// Whether `attempt` is still the current one for its profile. A
/// superseded generation loses even if it finished first. `settle`
/// enforces this inline; this is the assertion the tests read it with.
#[cfg(test)]
pub fn is_current(attempt: &Attempt) -> bool {
    table()
        .lock()
        .unwrap()
        .get(&attempt.profile_id)
        .and_then(|e| e.attempt.as_ref())
        .is_some_and(|a| a.generation == attempt.generation)
}

/// Records the end of an attempt.
///
/// `install` runs **under the lifecycle lock** and only when the attempt
/// is still current and not cancelled — that is the single point where a
/// provider may enter the live registry, so no stale or duplicate
/// session can replace the authoritative one. It returns whether the
/// installation happened; `false` leaves the previous session untouched.
pub fn settle<F>(attempt: &Attempt, outcome: Result<(), ConnFailure>, install: F) -> bool
where
    F: FnOnce(),
{
    let mut map = table().lock().unwrap();
    let Some(entry) = map.get_mut(&attempt.profile_id) else {
        return false;
    };
    let current = entry
        .attempt
        .as_ref()
        .is_some_and(|a| a.generation == attempt.generation);
    if !current {
        // A newer attempt owns this profile — this one's result is
        // dropped by the caller, which closes whatever it opened.
        return false;
    }
    let cancelled = attempt.cancelled();
    match outcome {
        Ok(()) if !cancelled => {
            install();
            *entry = Entry {
                phase: Phase::Connected,
                attempt: None,
                error: None,
            };
            true
        }
        Ok(()) => {
            // Late success for a cancelled attempt: the profile goes
            // back to a stable state and the provider is dropped.
            *entry = Entry {
                phase: Phase::Disconnected,
                attempt: None,
                error: None,
            };
            false
        }
        Err(failure) => {
            let phase = if failure.kind == FailureKind::Cancelled {
                Phase::Disconnected
            } else {
                Phase::Failed
            };
            *entry = Entry {
                phase,
                attempt: None,
                error: Some(failure),
            };
            false
        }
    }
}

/// Marks a profile as disconnecting (teardown in progress).
pub fn begin_disconnect(profile_id: &str) {
    let mut map = table().lock().unwrap();
    let entry = map.entry(profile_id.to_string()).or_default();
    entry.phase = Phase::Disconnecting;
    entry.error = None;
}

/// Final state after a teardown, a deletion, or a connection that was
/// found to be gone.
pub fn set_disconnected(profile_id: &str, error: Option<ConnFailure>) {
    let mut map = table().lock().unwrap();
    let entry = map.entry(profile_id.to_string()).or_default();
    entry.phase = if error.is_some() {
        Phase::Failed
    } else {
        Phase::Disconnected
    };
    entry.attempt = None;
    entry.error = error;
}

/// Drops a profile's record entirely (the profile itself is gone).
pub fn forget(profile_id: &str) {
    table().lock().unwrap().remove(profile_id);
}

/// The recorded phase, attempt id and error of a profile.
pub fn snapshot(profile_id: &str) -> (Phase, Option<String>, Option<ConnFailure>) {
    let map = table().lock().unwrap();
    match map.get(profile_id) {
        Some(e) => (
            e.phase,
            e.attempt.as_ref().map(|a| a.id.clone()),
            e.error.clone(),
        ),
        None => (Phase::Disconnected, None, None),
    }
}

/// Brings a record in line with what the registry actually holds. The
/// registry is the truth about a *live* session; the table adds the
/// states the registry cannot express (connecting, disconnecting,
/// failed). Only those two are reconciled, so a running attempt and a
/// recorded failure survive a refresh — which is what keeps the dialog
/// from resetting itself to "disconnected" when it reopens.
pub fn reconcile(profile_id: &str, live: bool) -> (Phase, Option<String>, Option<ConnFailure>) {
    let mut map = table().lock().unwrap();
    let entry = map.entry(profile_id.to_string()).or_default();
    match (entry.phase, live) {
        (Phase::Connected, false) => {
            entry.phase = Phase::Disconnected;
            entry.attempt = None;
        }
        (Phase::Disconnected | Phase::Failed, true) => {
            entry.phase = Phase::Connected;
            entry.error = None;
        }
        _ => {}
    }
    (
        entry.phase,
        entry.attempt.as_ref().map(|a| a.id.clone()),
        entry.error.clone(),
    )
}

// ---------- Transport ----------

/// Reaches the endpoint's port within [`TRANSPORT_TIMEOUT`], so a dead
/// host fails in 15 seconds with "unreachable" instead of sitting in
/// whatever default a protocol client happens to use. The probe socket
/// is closed again immediately; the protocol client opens its own.
///
/// `host` may carry a URL scheme (`http://dav.example`) — WebDAV
/// profiles are stored that way — which is stripped before resolving.
pub fn probe_transport(host: &str, port: u16) -> Result<(), ConnFailure> {
    let bare = host
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(host)
        .trim_end_matches('/');
    let target = format!("{}:{}", bare, port);

    let addrs = match target.to_socket_addrs() {
        Ok(a) => a.collect::<Vec<_>>(),
        Err(e) => {
            return Err(ConnFailure::from_error(
                FailureKind::Unreachable,
                AppError::new(code::NET_UNREACHABLE)
                    .op("connect")
                    .with("host", bare)
                    .action(ErrorAction::Retry)
                    .detail(e),
            ))
        }
    };
    if addrs.is_empty() {
        return Err(ConnFailure::from_error(
            FailureKind::Unreachable,
            AppError::new(code::NET_UNREACHABLE)
                .op("connect")
                .with("host", bare)
                .action(ErrorAction::Retry),
        ));
    }

    // Several addresses (IPv6 + IPv4) share the one budget: the limit is
    // on reaching the endpoint, not on each attempt at it.
    let each = TRANSPORT_TIMEOUT / addrs.len().min(4) as u32;
    let mut last = String::new();
    for addr in &addrs {
        match TcpStream::connect_timeout(addr, each) {
            Ok(_) => return Ok(()),
            Err(e) => last = e.to_string(),
        }
    }
    Err(ConnFailure::new(
        FailureKind::Unreachable,
        format!("{} nicht erreichbar: {}", target, last),
    ))
}

/// Classifies a protocol client's failure.
///
/// Until ticket 036 this had to guess from the words in a message,
/// because the protocol clients handed back prose. They now hand back a
/// typed `AppError`, so the kind is *read* rather than inferred — and
/// anything this does not recognize stays `Protocol` rather than being
/// guessed into something more specific.
pub fn classify_protocol_error(error: &AppError) -> ConnFailure {
    let kind = match &*error.code {
        code::NET_AUTH | code::CRED_WRONG_PASSWORD => FailureKind::Auth,
        code::NET_UNREACHABLE => FailureKind::Unreachable,
        code::NET_TIMEOUT => FailureKind::Timeout,
        code::CANCELLED => FailureKind::Cancelled,
        code::SECURITY_HOST_KEY_UNKNOWN | code::SECURITY_HOST_KEY_MISMATCH => FailureKind::HostKey,
        code::CRED_LOCKED | code::CRED_STORE | code::CRED_NOT_FOUND => FailureKind::Keychain,
        _ => FailureKind::Protocol,
    };
    ConnFailure::from_error(kind, error.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test uses its own profile id — the table is process-wide.
    fn id(name: &str) -> String {
        format!("{}-{}", name, next_generation())
    }

    #[test]
    fn a_fresh_profile_is_disconnected() {
        let (phase, attempt, error) = snapshot(&id("fresh"));
        assert_eq!(phase, Phase::Disconnected);
        assert!(attempt.is_none());
        assert!(error.is_none());
    }

    #[test]
    fn begin_registers_exactly_one_attempt() {
        let p = id("dup");
        let first = match begin(&p) {
            Start::Started(a) => a,
            Start::AlreadyRunning(_) => panic!("the first attempt must start"),
        };
        // A second click finds the running attempt, never a new socket.
        match begin(&p) {
            Start::AlreadyRunning(a) => assert_eq!(a.id, first.id),
            Start::Started(_) => panic!("a duplicate must not start a second attempt"),
        }
        assert_eq!(snapshot(&p).0, Phase::Connecting);
        assert_eq!(snapshot(&p).1.as_deref(), Some(first.id.as_str()));
        forget(&p);
    }

    #[test]
    fn different_profiles_run_independently() {
        let (a, b) = (id("indep-a"), id("indep-b"));
        let one = match begin(&a) {
            Start::Started(x) => x,
            _ => panic!(),
        };
        let two = match begin(&b) {
            Start::Started(x) => x,
            _ => panic!(),
        };
        assert_ne!(one.id, two.id);
        assert_ne!(one.generation, two.generation);
        cancel(&one.id);
        assert!(one.cancelled());
        assert!(
            !two.cancelled(),
            "cancelling one profile must not touch the other"
        );
        forget(&a);
        forget(&b);
    }

    #[test]
    fn cancel_is_idempotent_and_forgiving() {
        let p = id("cancel");
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        assert_eq!(cancel(&attempt.id).as_deref(), Some(p.as_str()));
        assert!(attempt.cancelled());
        // Again, and for something that never existed: both no-ops.
        cancel(&attempt.id);
        assert_eq!(cancel("conn-does-not-exist"), None);
        assert!(attempt.check().is_err());
        forget(&p);
    }

    #[test]
    fn a_cancelled_attempt_settles_to_disconnected_not_failed() {
        let p = id("cancel-settle");
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        cancel(&attempt.id);
        let installed = settle(
            &attempt,
            Err(ConnFailure::new(FailureKind::Cancelled, "abgebrochen")),
            || panic!("a cancelled attempt must not install a provider"),
        );
        assert!(!installed);
        assert_eq!(snapshot(&p).0, Phase::Disconnected);
        forget(&p);
    }

    #[test]
    fn a_late_success_for_a_cancelled_attempt_is_discarded() {
        let p = id("late-cancel");
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        cancel(&attempt.id);
        // The worker finished anyway — the provider must not be installed.
        let installed = settle(&attempt, Ok(()), || {
            panic!("a cancelled attempt must not reach the registry")
        });
        assert!(!installed);
        assert_eq!(snapshot(&p).0, Phase::Disconnected);
        forget(&p);
    }

    #[test]
    fn a_superseded_generation_never_wins() {
        let p = id("supersede");
        let old = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        // The old attempt is cancelled first (that is what disconnect and
        // a fresh connect do), so a new one may start.
        cancel(&old.id);
        settle(
            &old,
            Err(ConnFailure::new(FailureKind::Cancelled, "x")),
            || {},
        );
        let new = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        assert!(new.generation > old.generation);
        assert!(is_current(&new));
        assert!(!is_current(&old));
        // The old worker finishes late and must lose.
        assert!(!settle(&old, Ok(()), || panic!(
            "a stale generation must not install"
        )));
        assert_eq!(
            snapshot(&p).0,
            Phase::Connecting,
            "the new attempt still owns the profile"
        );
        forget(&p);
    }

    #[test]
    fn a_successful_attempt_installs_once_and_becomes_connected() {
        let p = id("success");
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        let mut installed = 0;
        assert!(settle(&attempt, Ok(()), || installed += 1));
        assert_eq!(installed, 1);
        assert_eq!(snapshot(&p).0, Phase::Connected);
        assert!(snapshot(&p).1.is_none(), "no attempt is pending any more");
        forget(&p);
    }

    #[test]
    fn a_failure_records_its_kind_and_leaves_no_attempt() {
        let p = id("failure");
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        settle(
            &attempt,
            Err(ConnFailure::new(FailureKind::Timeout, "zu langsam")),
            || panic!("a failed attempt must not install"),
        );
        let (phase, pending, error) = snapshot(&p);
        assert_eq!(phase, Phase::Failed);
        assert!(pending.is_none());
        assert_eq!(error.as_ref().unwrap().kind, FailureKind::Timeout);
        forget(&p);
    }

    #[test]
    fn reconcile_follows_the_registry_but_keeps_pending_and_failed() {
        let p = id("reconcile");
        // A live session the table did not know about.
        assert_eq!(reconcile(&p, true).0, Phase::Connected);
        // …and its loss.
        assert_eq!(reconcile(&p, false).0, Phase::Disconnected);

        // A running attempt survives a refresh (the dialog reopening
        // must not reset it to disconnected).
        let attempt = match begin(&p) {
            Start::Started(a) => a,
            _ => panic!(),
        };
        let (phase, pending, _) = reconcile(&p, false);
        assert_eq!(phase, Phase::Connecting);
        assert_eq!(pending.as_deref(), Some(attempt.id.as_str()));

        // …and so does a recorded failure.
        settle(
            &attempt,
            Err(ConnFailure::new(FailureKind::Auth, "nope")),
            || {},
        );
        let (phase, _, error) = reconcile(&p, false);
        assert_eq!(phase, Phase::Failed);
        assert_eq!(error.unwrap().kind, FailureKind::Auth);
        forget(&p);
    }

    #[test]
    fn disconnect_states_are_recorded() {
        let p = id("disconnect");
        reconcile(&p, true);
        begin_disconnect(&p);
        assert_eq!(snapshot(&p).0, Phase::Disconnecting);
        set_disconnected(&p, None);
        assert_eq!(snapshot(&p).0, Phase::Disconnected);
        // A connection found to be gone records why.
        set_disconnected(
            &p,
            Some(ConnFailure::new(
                FailureKind::Protocol,
                "Verbindung verloren",
            )),
        );
        let (phase, _, error) = snapshot(&p);
        assert_eq!(phase, Phase::Failed);
        assert_eq!(error.unwrap().kind, FailureKind::Protocol);
        forget(&p);
    }

    #[test]
    fn failures_carry_a_recognizable_marker() {
        // Since ticket 036 the failure carries a typed error rather
        // than a marker inside a sentence.
        let f = ConnFailure::new(FailureKind::Timeout, "zu langsam");
        assert!(f.error.is(code::NET_TIMEOUT));
        assert_eq!(f.error.detail.as_deref(), Some("zu langsam"));
        let cancelled = ConnFailure::new(FailureKind::Cancelled, "x");
        assert!(cancelled.error.is(code::CANCELLED));
        assert!(cancelled.error.action.is_none());
    }

    #[test]
    fn a_protocol_failure_is_read_from_its_code_not_guessed() {
        // Ticket 036 replaced the word-matching heuristic this used to
        // need: the provider hands back a typed error and the kind is
        // read off it.
        for (code, expected) in [
            (code::NET_AUTH, FailureKind::Auth),
            (code::CRED_WRONG_PASSWORD, FailureKind::Auth),
            (code::NET_UNREACHABLE, FailureKind::Unreachable),
            (code::NET_TIMEOUT, FailureKind::Timeout),
            (code::CANCELLED, FailureKind::Cancelled),
            (code::SECURITY_HOST_KEY_MISMATCH, FailureKind::HostKey),
            (code::CRED_LOCKED, FailureKind::Keychain),
        ] {
            let failure = classify_protocol_error(&AppError::new(code));
            assert_eq!(failure.kind, expected, "{}", code);
            assert!(failure.error.is(code), "the typed error survives: {}", code);
        }
    }

    #[test]
    fn an_unrecognized_code_stays_a_protocol_error() {
        // Never guessed into something more specific — including a code
        // from a newer backend this build does not know.
        for code in [code::FS_IO, "net.somethingNewerAddedLater"] {
            assert_eq!(
                classify_protocol_error(&AppError::new(code)).kind,
                FailureKind::Protocol,
                "{}",
                code
            );
        }
    }

    #[test]
    fn an_unresolvable_host_is_unreachable_not_a_protocol_error() {
        let err = probe_transport("host.invalid.example.test", 22).unwrap_err();
        assert_eq!(err.kind, FailureKind::Unreachable);
    }

    #[test]
    fn a_refused_port_is_unreachable() {
        // Port 0 is never listening; the probe must not hang on it.
        let err = probe_transport("127.0.0.1", 9).unwrap_err();
        assert_eq!(err.kind, FailureKind::Unreachable);
    }

    #[test]
    fn a_listening_port_passes_the_transport_probe() {
        // A server that accepts TCP but never speaks the protocol must
        // pass here — catching it is the *overall* timeout's job, not
        // the transport probe's. This pins that division of labour.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a probe target");
        let port = listener.local_addr().unwrap().port();
        assert!(probe_transport("127.0.0.1", port).is_ok());
        drop(listener);
    }

    #[test]
    fn a_url_scheme_in_the_host_is_stripped_before_resolving() {
        // WebDAV profiles store `http://host`; the probe resolves `host`.
        let err = probe_transport("http://host.invalid.example.test/", 8080).unwrap_err();
        // The host is a parameter now, not a word in a sentence.
        let host = err.error.params.get("host").map(String::as_str);
        assert_eq!(host, Some("host.invalid.example.test"));
    }

    #[test]
    fn the_timeouts_are_the_documented_constants() {
        assert_eq!(TRANSPORT_TIMEOUT, Duration::from_secs(15));
        assert_eq!(OVERALL_TIMEOUT, Duration::from_secs(45));
    }
}
