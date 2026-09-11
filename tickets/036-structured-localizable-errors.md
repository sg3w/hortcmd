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
contract. Diagnostics from TICKET-045 may retain redacted technical details, but
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

- [ ] Frontend decisions never depend on substring matching backend messages.
- [ ] Common failures render in German or English according to the active setting,
      without reconnecting or restarting.
- [ ] Batch failures identify their item and retain partial-success information.
- [ ] Unknown codes render a safe generic message and expose a correlation ID for
      diagnostics.
- [ ] Error payloads contain no secrets and technical details are not rendered by
      default.
- [ ] Cancellation, unsupported capability, authentication failure, host-key
      mismatch, checksum mismatch, and destination-space failure are distinct.
- [ ] Generated TypeScript bindings are current and checked in CI.

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
