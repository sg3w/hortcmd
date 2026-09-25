# TICKET-349: Reject an oversized archive sources list before per-source resolution

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/commands/fs/archive/pack.rs` (around lines 60-81), the
entries-count limit (`limits.entries`, default 100,000) is enforced for
recursively-discovered directory children (`pack.rs:103`) but not for the
initial `sources` list itself before per-source `symlink_metadata`/
`canonicalize`/capability work runs.

The MCP `create_archive` schema caps `sources` only with `minItems: 1`, no
upper bound (`mcp/tools.rs:610`). A caller passing a very large `sources`
array of existing paths makes the first loop in `pack::run_with_recovery`
(lines 61-81) do a `caps::require` + `vfs::resolve` + `symlink_metadata` +
`canonicalize` for every one of them before the while-loop's `manifest.add`
(which does enforce the count cap) ever gets a chance to reject the job — all
that I/O happens regardless of the 100,000-entry limit.

## Required change

- Reject up front with `safety::limit()` if `sources.len() as u64 >
  limits.entries`, before the per-source resolution loop, mirroring the
  check already done for recursive children at `pack.rs:103`.

## Verification

- Test: call `create_archive` with a `sources` array larger than
  `limits.entries`; assert it's rejected immediately without per-source I/O.
- `npm run check`.
