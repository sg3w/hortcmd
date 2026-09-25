# TICKET-310: Clear the secret field when auth method or protocol changes

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src/features/net/ConnectionsDialog.tsx`, the single `secret` field state
used for both "password" and "key passphrase" (around line 317, state updates
at 326-343) is never cleared when the user switches `auth` or `protocol`
mid-edit. `set("auth", …)` and `setProtocol()` only reset `security`, never
`secret`.

In "Add connection", if a user leaves protocol at `sftp`/auth `password`,
types their login password, then switches Auth to "Private key", the
Passphrase field (bound to the same `secret` state) is now pre-filled with
that password text. `submit()` sends it straight through as `secretToSave` via
`netSaveProfile` — the user can save their login password as the SSH key
passphrase, or vice versa, without ever typing into that field.

Not covered by `ConnectionsDialog.test.tsx`, whose parametrized secret tests
fix `auth` for the whole test rather than switching it mid-form.

## Required change

- Clear `secret` (`setSecret("")`) inside the `auth`/protocol-change handlers,
  the same way `security` is already reset on host/port/user changes.

## Verification

- New test: fill the password field, switch Auth to "Private key", assert the
  passphrase field is empty.
- `npm run check`.
