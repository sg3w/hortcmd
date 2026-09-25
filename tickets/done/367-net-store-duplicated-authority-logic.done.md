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

## Resolution (2026-09-26)

`applyStates` now compares `authorityOf(panePath)` with
`authorityOf(c.address)` instead of rebuilding `scheme://key/` from the
connection key, so the pane-to-connection match lives in exactly one place and
follows any future change to the backend's `remote_uri` format. As a side
effect the match is now case-insensitive like `connectionFor`, so a pane path
whose host differs only in case is refreshed too. A lifecycle test pins the
behaviour: a restored connection refreshes the pane on its authority (spelled
with different case) and leaves a pane on another host alone.

Validation: `npm run check` passed on macOS.
