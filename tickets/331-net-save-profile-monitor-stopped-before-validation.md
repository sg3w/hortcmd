# TICKET-331: Don't stop a profile's monitor before its save can still fail

Discovered during a full-codebase review (2026-09-22).

## Problem

`net_save_profile` (`src-tauri/src/net/session.rs`, around lines 455-463)
stops its live monitor/auto-reconnect (`monitor::stop_profile(&profile.id)`,
line 455) *before* the validation steps that can still fail — a profile with
`monitor.auto_reconnect = true` that is currently connected and being watched
gets its monitor unconditionally killed. The function then continues to
`super::certificate::approved_der(&profile)?` (line 458) or
`credentials::store().set(...)?` (line 463), either of which can legitimately
fail (e.g. the profile's pinned certificate file was deleted/rotated on disk,
or the configured KeePass store is locked).

On that failure the command returns `Err` and the in-memory `Profile`/
registry are untouched, but the monitor that was protecting the still-live
connection is already gone and nothing restarts it. If the connection later
drops, it will not auto-reconnect, with no indication to the user beyond an
unrelated "save failed" error.

## Required change

- Defer `monitor::stop_profile` until after all fallible validation
  (`approved_der`, `credentials::store().set`, `store_profiles`) has
  succeeded, or restart the monitor on any early return so a failed save can
  never leave a previously-watched connection unmonitored.

## Verification

- Test: edit a monitored, connected profile such that a later validation
  step fails (e.g. mocked credential store failure); assert the monitor is
  still running afterward.
- `npm run check`.
