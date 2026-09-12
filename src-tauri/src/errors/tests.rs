// ============================================================
// The error contract (ticket 036).
//
// Three things are worth pinning: an OS error becomes the right code
// regardless of which provider raised it, a code stays a code (control
// flow must never come back to matching text), and — the one that
// would be a real incident — nothing secret survives redaction.
// ============================================================

use super::*;

// ---------- Mapping ----------

#[test]
fn os_error_kinds_map_to_their_codes() {
    for (kind, expected) in [
        (io::ErrorKind::NotFound, code::FS_NOT_FOUND),
        (io::ErrorKind::PermissionDenied, code::FS_PERMISSION),
        (io::ErrorKind::AlreadyExists, code::FS_EXISTS),
        (io::ErrorKind::Interrupted, code::CANCELLED),
        (io::ErrorKind::UnexpectedEof, code::FS_IO),
    ] {
        let err: AppError = io::Error::new(kind, "x").into();
        assert!(err.is(expected), "{:?} → {}", kind, err.code);
    }
}

#[test]
fn a_cancel_is_not_a_failure() {
    // The transfer engine signals a cancel with `Interrupted`; it must
    // not arrive in the UI as an error.
    let err: AppError = io::Error::new(io::ErrorKind::Interrupted, "abgebrochen").into();
    assert!(err.is(code::CANCELLED));
    assert!(
        err.action.is_none(),
        "a cancel offers the user nothing to do"
    );
    assert!(!err.retryable);
}

#[test]
fn the_action_follows_from_the_kind() {
    let denied: AppError = io::Error::new(io::ErrorKind::PermissionDenied, "x").into();
    assert_eq!(denied.action, Some(ErrorAction::CheckPermissions));
    let busy: AppError = io::Error::other("x").into();
    assert_eq!(busy.action, Some(ErrorAction::Report));
}

#[test]
fn a_legacy_string_becomes_detail_never_a_message() {
    // The migration path: prose from a provider is kept for diagnostics
    // and the user sees a generic sentence instead.
    let err: AppError = "Verbindung fehlgeschlagen: 530".to_string().into();
    assert!(err.is(code::INTERNAL_UNEXPECTED));
    assert_eq!(
        err.detail.as_deref(),
        Some("Verbindung fehlgeschlagen: 530")
    );
    assert!(err.params.is_empty(), "nothing display-ready in the params");
}

#[test]
fn every_error_carries_a_version_and_a_correlation_id() {
    let a = AppError::new(code::FS_IO);
    let b = AppError::new(code::FS_IO);
    assert_eq!(a.version, ERROR_CONTRACT_VERSION);
    assert!(a.id.starts_with('e'));
    assert_ne!(a.id, b.id, "two errors must be distinguishable in a report");
}

#[test]
fn the_builder_describes_rather_than_phrases() {
    let err = AppError::new(code::NET_AUTH)
        .op("connect")
        .provider("sftp")
        .with("host", "example.com")
        .action(ErrorAction::Authenticate)
        .retryable();
    assert!(err.is(code::NET_AUTH));
    assert_eq!(err.op.as_deref(), Some("connect"));
    assert_eq!(err.provider.as_deref(), Some("sftp"));
    assert_eq!(err.params["host"], "example.com");
    assert!(err.retryable);
}

#[test]
fn codes_are_unique_and_namespaced() {
    let mut seen = std::collections::HashSet::new();
    for c in code::ALL {
        assert!(seen.insert(*c), "duplicate code: {}", c);
        assert!(
            c.contains('.') || *c == code::CANCELLED,
            "code without a namespace: {}",
            c
        );
    }
}

// ---------- Redaction ----------

/// Everything a secret can look like on the way into an error.
const SECRETS: [&str; 6] = [
    "hunter2",
    "s3cr3t-token",
    "abc.def.ghi",
    "MY-API-KEY-123",
    "correct horse battery staple",
    "swordfish",
];

fn leaks(text: &str) -> Option<&'static str> {
    SECRETS.into_iter().find(|s| text.contains(s))
}

#[test]
fn url_user_info_loses_its_password_and_keeps_its_user() {
    let out = redact("sftp://alice:hunter2@example.com/srv/data");
    assert_eq!(out, "sftp://alice:***@example.com/srv/data");
    // The user name is not a secret and is what makes the message useful.
    assert!(out.contains("alice"));
    assert!(out.contains("example.com"));
}

#[test]
fn a_url_without_a_password_is_left_alone() {
    assert_eq!(
        redact("sftp://alice@example.com/srv"),
        "sftp://alice@example.com/srv"
    );
    assert_eq!(redact("https://example.com/a/b"), "https://example.com/a/b");
}

#[test]
fn assignments_lose_their_value_in_every_shape() {
    for text in [
        "password=hunter2",
        "password: hunter2",
        r#"Passwort "hunter2""#,
        "?token=s3cr3t-token&x=1",
        "apikey = MY-API-KEY-123",
        "passphrase: correct horse battery staple",
    ] {
        let out = redact(text);
        assert!(leaks(&out).is_none(), "leaked in {:?}: {}", text, out);
    }
}

#[test]
fn bearer_tokens_and_key_blocks_are_removed() {
    let out = redact("Authorization: Bearer abc.def.ghi");
    assert!(leaks(&out).is_none(), "{}", out);

    let pem = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\nswordfish\n-----END OPENSSH PRIVATE KEY-----\nafter";
    let out = redact(pem);
    assert!(leaks(&out).is_none(), "{}", out);
    assert!(out.contains("before") && out.contains("after"));
}

#[test]
fn redaction_survives_nesting_and_repetition() {
    // A wrapped source error can carry the same secret several times,
    // in different shapes.
    let text = "connect to sftp://bob:hunter2@host failed: \
                auth error (password=hunter2, token=s3cr3t-token)";
    let out = redact(text);
    assert!(leaks(&out).is_none(), "{}", out);
    assert!(out.contains("host"), "the useful part survives: {}", out);
}

#[test]
fn redaction_runs_on_the_way_into_a_payload() {
    // Not at the call sites — here, so one forgetful caller is not a leak.
    let err = AppError::new(code::NET_AUTH)
        .with("address", "sftp://bob:hunter2@host/srv")
        .detail("login failed for password=hunter2");
    assert!(leaks(&err.params["address"]).is_none(), "{:?}", err.params);
    assert!(
        leaks(err.detail.as_deref().unwrap()).is_none(),
        "{:?}",
        err.detail
    );
}

#[test]
fn redaction_leaves_ordinary_text_intact() {
    for text in [
        "/Users/jörg/Dokumente/Bericht 2026.pdf",
        "C:\\Users\\x\\a.txt",
        "sftp://example.com:2222/srv",
        "no such file or directory",
    ] {
        assert_eq!(redact(text), text, "changed: {}", text);
    }
}

#[test]
fn redaction_is_idempotent() {
    let once = redact("password=hunter2 at sftp://a:b@h/x");
    assert_eq!(redact(&once), once);
}

// ---------- Round trip through io::Error ----------

#[test]
fn a_code_survives_the_provider_trait() {
    // Providers speak `io::Result`; an AppError passing through must not
    // come back as a generic I/O failure.
    let original = AppError::new(code::FS_NO_SPACE).detail("disk full");
    let io_err: io::Error = original.clone().into();
    assert!(io_err.to_string().contains(code::FS_NO_SPACE));
    assert!(io_err.to_string().contains(&*original.id));
}

#[test]
fn the_error_stays_small_enough_for_every_result_to_carry_it() {
    // Every `AppResult<T>` in the crate is as large as this. Clippy's
    // `result_large_err` threshold is 128 bytes, and staying under it is
    // why the string fields are boxed.
    assert!(
        std::mem::size_of::<AppError>() <= 128,
        "AppError is {} bytes",
        std::mem::size_of::<AppError>()
    );
}
