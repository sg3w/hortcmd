# TICKET-341: Give the SMB share-listing failure its own error code

Discovered during a full-codebase review (2026-09-22).

## Problem

`shares()` in `src-tauri/src/vfs/smb.rs` (around lines 200-208) builds a
carefully worded, actionable error message ("this server doesn't support
share listing, set a start path with a share, e.g. /Freigabe") and returns it
as a bare `String`, which `?` converts via `impl From<String> for AppError`
into `code::INTERNAL_UNEXPECTED` with the whole message stuffed into
`.detail`. Per the error contract, `.detail` is diagnostics-only and not
rendered by default.

A Samba server that rejects the share-enumeration RPC bind (the exact case
this message was written for) causes the user to see only the frontend's
generic fallback sentence for `internal.unexpected` — the specific, useful
guidance about setting a start path never reaches them, defeating the point
of writing it.

## Required change

- Give this case its own error code with the guidance passed as a safe
  `.with(...)` param (or a dedicated capability/action), rather than routing
  it through the generic `AppError::from(String)` migration path meant for
  not-yet-improved prose.

## Verification

- Test: an SMB server that rejects share enumeration; assert the surfaced
  error is the dedicated code with the actionable guidance visible, not the
  generic internal-error fallback.
- `npm run test:harness` (SMB), `npm run check`.

## Resolution (2026-09-27)

The share-enumeration failure has its own code, `net.shareListingUnsupported`,
instead of a prose `String` that `From<String>` turned into
`internal.unexpected` with the guidance buried in the diagnostics-only
`.detail`. `shares()` now builds
`AppError::new(code::NET_SHARE_LISTING_UNSUPPORTED).provider("smb").op("list")
.action(ErrorAction::Unsupported).detail(e)`, so the raw protocol error stays
where diagnostics want it and the actionable sentence — set a start path that
includes a share, e.g. /Freigabe — is the displayed message, translated in both
languages like every other code.

The dedicated code is in `errors::code::ALL`, so the generated
`errorCodes.generated.ts` and the translation test cover it; a Samba server
rejecting the RPC bind now reaches the user with the guidance rather than the
generic internal-error fallback. Asserting that end to end needs a server that
rejects share enumeration, which stays with the SMB harness tickets.

Validation: `npm run check` passed on macOS.
