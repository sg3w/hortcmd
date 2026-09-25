# TICKET-367: Reuse authorityOf instead of re-deriving the address prefix

Discovered during a full-codebase review (2026-09-22).

## Problem

`applyStates` in `src/features/net/netStore.ts` (lines 276-289) hand-builds
the address prefix as `` `${c.address.split("://")[0]}://${c.key}/` `` to
match a pane's path, duplicating the authority-matching logic already
centralized in `authorityOf`/`connectionFor` a few dozen lines below (lines
411-429) in the same file.

Not incorrect today (confirmed backend format is `scheme://key/path`), but
the duplicated string-building is a second place that must stay in sync with
the backend's `remote_uri` format — a future change to that format only
needs updating in one of the two spots to silently break the other.

## Required change

- Reuse `authorityOf(c.address)` for the prefix check instead of
  reconstructing it from `c.key`.

## Verification

- Existing net store tests continue to pass unchanged after the refactor.
- `npm run check`.
