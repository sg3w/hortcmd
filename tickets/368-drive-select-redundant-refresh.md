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
