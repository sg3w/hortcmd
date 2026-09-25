# TICKET-368: Move DriveSelect's connection refresh to a single app-level effect

Discovered during a full-codebase review (2026-09-22).

## Problem

`useEffect(() => { void useNet.getState().refresh(); }, [])` in
`src/features/panel/DriveSelect.tsx` (lines 38-40) runs once per mounted
`DriveSelect`, and there are always two (left/right pane), so app startup
issues two redundant `netListProfiles`/`netListConnections` round trips
instead of one.

## Required change

- Move this refresh to a single app-level effect (e.g. alongside
  `useRefresh`) rather than per-pane.

## Verification

- Manual/network-log check that app startup issues one
  `netListProfiles`/`netListConnections` round trip instead of two.
- `npm run check`.

## Resolution (2026-09-26)

`DriveSelect` no longer fetches the connection snapshot itself. The single
app-level `useRefresh` effect now calls `useNet.refresh()` once on startup, so
both pane pickers read the same store state and later changes still arrive
through the store rather than through another fetch. A `useRefresh` test mounts
the hook and both `DriveSelect` instances and asserts exactly one
`net_list_profiles`/`net_list_connections` round trip; it fails on the previous
per-pane effect.

Validation: `npm run check` passed on macOS.
