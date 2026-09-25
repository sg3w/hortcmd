# TICKET-312: Re-check MCP scope/permission during archive extraction

Discovered during a full-codebase review (2026-09-22).

## Problem

`extract::run_with_recovery` (`src-tauri/src/commands/fs/archive/extract.rs`,
around line 400) has no live-authorization hook, unlike
`pack::run_with_recovery` (`pack.rs:37`). `mcp::transfers::extract`
(`mcp/transfers.rs:123-143`) calls `extract::run(job, &mut prog,
Limits::load()?)` with no such callback — only a plain cancellation flag and a
one-time `caps::require_pair` at the start.

`copy`, `move`, and `create_archive` in `mcp/transfers.rs` all thread a check
tied to `execution::live_config()` (via a `ScopedProvider`/explicit `check`
closure) into every file operation, so revoking MCP write access or narrowing
`roots` mid-operation aborts and cleans up (exercised by
`cancelled_copy_and_move_clean_only_their_stage_and_report_revoked_cleanup`).
A large archive extraction (budget up to 1800s) that starts while a directory
is in scope, then has that directory revoked mid-run, keeps writing files into
it and reports success — the one of the four MCP file operations that isn't
continuously re-authorized.

## Required change

- Give `extract::run_with_recovery` a `check: &dyn Fn(&Path) -> AppResult<()>`
  parameter mirroring `pack::run`'s signature, invoked per-entry before
  `write_entry`/`dest.directory`.
- Have `mcp::transfers::extract` pass a `ScopedProvider`-backed closure like
  `create_archive` already does.

## Verification

- New MCP test mirroring the existing cancelled-copy/move revoked-cleanup
  test, but for extraction: revoke scope mid-extract and assert the operation
  aborts and cleans up its staged output.
- `npm run check`.
