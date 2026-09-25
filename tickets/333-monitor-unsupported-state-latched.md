# TICKET-333: Clear the connection monitor's Unsupported state once healthy

Discovered during a full-codebase review (2026-09-22).

## Problem

In `supervise()` (`src-tauri/src/net/session/monitor.rs`, around lines
182-199), once a health probe reports `Health::Unsupported`,
`MonitorState::Unsupported` is latched and never cleared even after later
probes succeed (`Health::Alive`), because only the full "lost → reconnect →
restored" path resets the state to `Watching`.

Connecting an SMB profile without browsing into any share yet:
`SmbProvider::health_check` returns `Health::Unsupported` whenever no share is
attached, which it is not at first — the monitor tick sets
`MonitorState::Unsupported` and pushes it to the frontend. The user then
browses into a share, so `self.connected` gains an entry and subsequent
`health_check` calls return `Health::Alive`. `ProbeFailures::lost` resets its
failure counter to 0 and `continue`s without ever calling `update(&app,
&control, MonitorState::Watching)`. The connection manager keeps showing
"Unsupported" indefinitely for a connection that is healthy and being
actively monitored.

## Required change

- When `health_check` returns something other than `Unsupported` and the
  previous state was `Unsupported`, call `update(&app, &control,
  MonitorState::Watching)` (or otherwise reconcile `control.state` every tick
  instead of only on the two explicit paths).

## Verification

- Test: connect an SMB profile without browsing a share (Unsupported), then
  browse into a share; assert the monitor state transitions back to
  Watching.
- `npm run check`.
