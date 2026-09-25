# TICKET-324: Gate read_file_checksums on Capability::Checksums

Discovered during a full-codebase review (2026-09-22).

## Problem

`read_file_checksums` in `src-tauri/src/commands/fs/props.rs` (around line
281) never calls `caps::require(&path, Capability::Checksums)`, unlike every
sibling in this file (`read_file_props` at line 150, `set_permissions` at
line 200, `set_owner` at line 227) and unlike the module's own documented
invariant that commands refuse an address lacking the capability they need.

`Capability::Checksums` is explicitly a local-only capability (not in
`remote_core_caps()`, `vfs/caps.rs:489`), yet `read_file_checksums` resolves
via the generic `vfs::resolve` and works against any provider. It is reachable
both as the `file_checksums` Tauri command and directly from
`mcp/tools.rs:233-236`'s `checksums()` tool — which, unlike its neighbors
`properties()`, `volume_info()`, `git_status()`, and `compare_dirs()` in the
same file (all of which call `caps::require`/`require_pair` first), performs
no capability check at all.

An MCP client (or any caller reaching this path) requesting checksums for a
remote/archive address the capability matrix says must not support it
succeeds silently instead of being refused, streaming a whole remote file
synchronously with no capability gate.

## Required change

- Add `caps::require(&path, Capability::Checksums)?;` at the top of
  `read_file_checksums`, mirroring `read_file_props`.

## Verification

- Test that `read_file_checksums`/the `checksums` MCP tool refuses a remote
  address the same way `read_file_props` does.
- `npm run check`.
