# TICKET-363: Cache mount-table/ancestor lookups within one icon batch

Discovered during a full-codebase review (2026-09-22).

## Problem

`local_icon_path` in `src-tauri/src/commands/fs/icons/macos.rs` (lines
23-24, 32-40) re-fetches the full mount table (`icon_mounts()`, two
`getfsstat` calls) and re-`symlink_metadata`s every ancestor directory of the
path, independently for every single icon request, with no caching across a
batch.

`file_icons` processes up to 16 requests per batch sequentially on the single
icon worker (`icons.rs`). Scrolling a large, deeply-nested folder repeatedly
re-validates the same ~10-20 ancestor directories and re-reads the mount
table for every sibling file in that same folder, multiplying syscalls by the
batch size for work whose result is identical within one batch/directory.

## Required change

- Cache the mount table and the "clean ancestor chain" result per unique
  parent directory for the lifetime of one `file_icons` call (or one
  `spawn_blocking` batch) instead of recomputing per request.

## Verification

- Existing icon tests continue to pass; optionally a test/benchmark showing
  reduced syscall count for a batch of icons from the same directory.
- `npm run check`.

## Resolution (2026-09-25)

The macOS icon worker now owns metadata for one batch: the mount table is fetched lazily once and parent-chain checks are reused for sibling requests. Every leaf is still checked, remote mounts are refused, and nothing survives into the next batch. A mocked metadata regression verifies reduced lookup counts and retained refusal behavior without native GUI testing.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
