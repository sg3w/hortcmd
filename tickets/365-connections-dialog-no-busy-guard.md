# TICKET-365: Add a busy guard to ConnectionsDialog's Save/Apply actions

Discovered during a full-codebase review (2026-09-22).

## Problem

`ProfileForm.submit()` (`src/features/net/ConnectionsDialog.tsx`, lines
350-356/493-499) and `CredentialStoreForm.apply()` (lines 537-544/632-639)
have no busy/in-flight guard, unlike every other action in the same file
(`UnlockForm`, `ForgetHostKeyPanel`, `CertificateTrustForm`,
`TransportSecurityForm` all track `busy` and disable their buttons/no-op on
re-entry).

A user double-clicks "Save" (or "Apply") before the first
`save()`/`setCredBackend()` promise resolves — nothing disables the button
(`disabled={!canSave}` only checks form validity) — firing two concurrent
`netSaveProfile`/`netCredSetBackend` IPC calls that race to set
`editing`/`panel` state.

## Required change

- Add the same local `busy` pattern used by the sibling panels in this file
  and disable Save/Apply while the call is in flight.

## Verification

- Test: double-click Save/Apply before the IPC call resolves; assert only
  one call fires.
- `npm run check`.
