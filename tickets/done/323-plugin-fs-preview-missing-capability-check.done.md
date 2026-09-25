# TICKET-323: Gate the plugin fs:preview request on Capability::Preview

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/plugins/filesystem.rs` (around lines 56-62),
`PluginRequest::Preview` (`fs:preview`) is the only filesystem request type
that skips the `caps::require(path, Capability::…)` check that its siblings
`fs:read` (`Capability::Preview`) and `fs:list` (`Capability::Browse`) both
perform before touching the VFS.

A plugin holding only `read:fs` (the same permission `fs:read`/`fs:list`/
`op:open` require) can call `hortcmd.request({type:'fs:preview', path})` on
any address that resolves without going through the location-capability gate
that every other read path enforces. Today this is inert because
`local_caps()`/`remote_core_caps()` in `src-tauri/src/vfs/caps.rs` currently
always bundle `Preview` together with `Browse` for every supported scheme —
so no location exists today where `Browse` succeeds but `Preview` is
withheld. The gap becomes exploitable the moment a location's capability set
diverges (e.g. a future read-only/archive-scoped location that advertises
`Browse` but not `Preview`), silently letting `fs:preview` bypass a
restriction `fs:read` still enforces.

## Required change

- Add `caps::require(path, Capability::Preview)` to the `Preview` arm in
  `filesystem.rs::execute`, mirroring the `Read` arm.

## Verification

- Test with a mocked provider whose capability set has `Browse` but not
  `Preview`; assert `fs:preview` is refused the same way `fs:read` would be.
- `npm run check`.

## Resolution (2026-09-25)

Plugin fs:preview now requires the Preview capability before reading the file. A browse-only capability fixture refuses both preview and read requests; a normal local preview still succeeds.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
