# TICKET-036: Introduce structured and localizable application errors

## Goal

Replace display-ready backend error strings with stable typed error codes,
parameters, and optional technical context so users receive messages in their
selected language and callers can react without parsing text.

## Background

Rust commands currently return many plain `String` errors and operation results
contain `Vec<String>`. Several messages are German, while the frontend also tests
sentinel text to identify archive-password failures. Network stores, transfers,
and local filesystem commands expose raw platform or protocol wording through the
same channel. This creates inconsistent localization, brittle control flow, and a
risk of leaking credentials, URLs, or sensitive paths in dialogs and future logs.

The generated ts-rs bindings provide a natural place for a versioned error
contract. Diagnostics from TICKET-109 may retain redacted technical details, but
user-facing messages must be derived from stable fields.

## Requirements

- Define a serializable `AppError` contract with at least:
  - a stable namespaced code;
  - safe localization parameters;
  - operation and provider context where applicable;
  - retryability and user-action hints;
  - an optional redacted technical detail intended for diagnostics, not display.
- Export the contract and relevant enums to TypeScript through ts-rs.
- Establish error-code namespaces for filesystem, transfer, archive, network,
  credential, security, capability, validation, cancellation, and internal errors.
- Map common `std::io`, provider, SSH, HTTP/WebDAV, FTP, SMB, keyring, archive, and
  Tauri failures at their owning boundary.
- Preserve per-item errors for batch operations without losing the affected item
  or successful-item count.
- Replace archive-password string sentinels with codes.
- Translate codes in the frontend using typed dictionary entries and safe fallback
  text for an unknown code from a newer backend.
- Keep cancellation distinct from failure and do not show it as an error.
- Redact passwords, passphrases, bearer tokens, URL user information, and other
  secrets before values enter an error payload.
- Provide a compatibility migration plan for commands consumed by the planned MCP
  and plugin interfaces; do not require those callers to parse localized text.

## Acceptance Criteria

- [x] Frontend decisions never depend on substring matching backend messages.
- [x] Common failures render in German or English according to the active setting,
      without reconnecting or restarting.
- [x] Batch failures identify their item and retain partial-success information.
- [x] Unknown codes render a safe generic message and expose a correlation ID for
      diagnostics.
- [x] Error payloads contain no secrets and technical details are not rendered by
      default.
- [x] Cancellation, unsupported capability, authentication failure, host-key
      mismatch, checksum mismatch, and destination-space failure are distinct.
- [x] Generated TypeScript bindings are current and checked in CI.

## Tests

- Unit-test error mapping for representative OS error kinds and every remote
  provider family.
- Snapshot localization for every public error code in German and English and fail
  on missing dictionary entries.
- Test archive-password flows without sentinel strings.
- Test mixed-success batch operations with multiple item failures.
- Fuzz or property-test redaction against credential-bearing URLs, headers, and
  nested source errors.
- Test compatibility fallback with an unknown code and additional unknown fields.
- Run the complete checks introduced by TICKET-033.

## Definition of Done

- A versioned typed error contract crosses all production IPC boundaries.
- User-facing backend errors are localized in the frontend and control flow uses
  codes rather than text.
- Secret redaction is centralized and tested.
- Public error codes and compatibility rules are documented for future consumers.
- No raw provider error is displayed without safe mapping.
- Concept.md documents the error boundary and ownership of localization.
- Rename this file to `036-structured-localizable-errors.done.md`.

---

## Implementation

### The contract

`src-tauri/src/errors/mod.rs` defines `AppError`: a versioned payload with a
stable namespaced `code`, safe localization `params`, `op`/`provider` context,
`retryable`, an `action` hint (`ErrorAction`), a redacted `detail` for
diagnostics, and a correlation `id`. `AppResult<T>` replaced
`Result<T, String>` across every command — 31 signatures, plus the `Provider`
trait and the transfer engine's internals.

45 codes in ten namespaces: `fs.` `transfer.` `archive.` `net.` `cred.`
`security.` `capability.` `validation.` `cancelled` `internal.`. They are
`&'static str` rather than an enum so a newer backend can introduce one an older
frontend renders generically — that is the compatibility rule the planned MCP
and plugin consumers inherit.

### Mapping at the owning boundary

`From<io::Error>` serves every filesystem path, so "not found" reads the same
whether it came from a local `stat` or an SFTP `open`. `From` impls map
ZipError, opener, trash, plist, serde_json, tauri and russh; the keyring,
KeePass, SSH, archive and capability boundaries map their own outcomes
explicitly. `From<String>` is the migration path: prose becomes *detail*, never
the displayed message, so nothing that still produces a sentence can leak it.

### Sentinels replaced by codes

| was | now |
|---|---|
| `PASSWORD_REQUIRED` / `PASSWORD_WRONG` in message text | `archive.passwordRequired` / `archive.passwordWrong` |
| `KEEPASS_LOCKED` marker | `cred.locked` |
| `UNSUPPORTED_LOCATION` marker | `capability.unsupported` / `capability.offline` (the distinction is real: one can be reconnected) |
| `CONN_*` tags on a thrown string | the typed error on `ConnFailure` |

`net::lifecycle::classify_protocol_error` no longer guesses a kind from the
words in a message — it reads the code, which is exactly what that function's
own comment said this ticket would do.

### Redaction

Centralized in `redact`, applied to everything entering a payload: URL user-info
(keeping the user name, dropping the secret), `password=`/`token=` assignments in
any shape, bearer tokens, and pasted key blocks. Idempotent, and left to
recurse-safe iteration after the first draft's recursive `redact_bearer`
overflowed the stack on its own output — caught by its test.

### Batches

`OpResult`/`OpDone` carry `Vec<ItemError>`: each failure named with the item it
belongs to, alongside the `ok` count. A cancelled item is filtered out of what
is shown — a cancel is not a failure.

### Frontend

- `src/lib/appError.ts` — pure: `isAppError`, `asAppError`, `isError`,
  `isOneOf`, `isCancelled`, `fillParams`. Usable from anywhere, including `lib`.
- `src/store/errorText.ts` — localized: `describeError`, `describeErrorFull`,
  `errorReference`, `errorActionLabel`, `describeItemErrors`. Needs the language
  setting, which is why it is *not* in `lib/` — the boundary check caught that
  and the split is the better shape.
- `src/i18n/namespaces/errors.ts` — one entry per code, both languages, plus the
  frame (`error.unknown`, `error.reference`, `error.item`) and the action labels.

### Size

`AppError`'s string fields are `Box<str>`: at 152 bytes the struct made every
`AppResult<T>` in the crate large enough for Clippy's `result_large_err` to fire
110 times. Boxing brings it under 128 with an identical wire shape; a test pins
the size so a new field cannot quietly undo it.

### Tests

- Rust (`errors/tests.rs`, 21): OS-error kinds → codes; cancellation distinct
  and action-free; the action following from the kind; a legacy string becoming
  detail and never a message; version and a unique correlation id per error;
  unique namespaced codes; the size bound; and redaction against URL user-info,
  six assignment shapes, bearer tokens, key blocks, nested/repeated secrets,
  ordinary text left intact, and idempotence.
- Rust: the lifecycle classifier reading codes instead of guessing, and an
  unrecognized code (including one from a "newer backend") staying `protocol`.
- Frontend (`store/errorText.test.ts`, 24): shape recognition; exact code
  matching (a prefix must not match); cancellation kept distinct; **every code in
  the generated list translated in both languages, each with its own wording and
  matching placeholders**; language switching re-rendering; parameters filled and
  a missing one left visible; an unknown code rendering generically *with* its
  reference; unknown extra fields tolerated; detail never displayed; every action
  labelled; and a mixed-success batch naming each item, dropping cancellations,
  and handling an operation-level failure with no item name.

### Also changed

`scripts/check-bindings.mjs` compared `git status`, so it refused to run on a
branch where the bindings were legitimately being changed — precisely when it
matters. It now snapshots, regenerates and compares contents.

### Verified

`npm run check` exits 0: 268 Rust tests, 303 frontend tests, `cargo fmt`, strict
Clippy with no allowances, generated bindings and error codes current,
boundaries clean, bundle within budget. In the browser demo, seven
representative errors were rendered in both languages: parameters filled
(`nas.local ist nicht erreichbar` / `nas.local is not reachable`), `detail`
suppressed (a `530 Login incorrect` behind `net.auth` shows only "Anmeldung
abgelehnt"), and an unknown code degrading to
`Unerwarteter Fehler (net.somethingAddedLater)` with its reference. Console
clean.

### Not done

Diagnostics that *would* surface `detail` are TICKET-109's scope; the field is
carried and redacted but nothing renders it yet. A handful of internal helpers
still produce prose that arrives as `internal.unexpected` with the text as
detail — safe by construction, and each one is a local improvement to make
rather than a leak.
