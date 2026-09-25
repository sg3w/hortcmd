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
