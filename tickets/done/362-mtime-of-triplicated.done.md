# TICKET-362: Share one mtime-from-Metadata helper across walk/compare/props

Discovered during a full-codebase review (2026-09-22).

## Problem

The exact same 4-line "Unix-seconds mtime from `Metadata`" logic
(`meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)`)
is implemented three separate times: as the named helper `walk::mtime_of`
(`src-tauri/src/commands/fs/walk.rs:119-125`, private, not reusable as-is),
inlined in `compare::read_level` (`compare.rs:76-81`), and inlined again in
`props::read_file_props` (`props.rs:178-183`).

Any future fix to this logic (e.g. handling pre-1970 timestamps, or platform
quirks) has to be found and applied in three places; it's easy to fix one and
miss the others.

## Required change

- Make `walk::mtime_of` `pub(crate)` (or move it to a shared `fs`-utility
  module) and have `compare.rs`/`props.rs` call it instead of reimplementing
  it.

## Verification

- `cargo build`/`npm run check` after the refactor; existing walk/compare/
  props tests continue to pass unchanged.

## Resolution (2026-09-25)

Directory comparison and file properties now call walk::mtime_of for the shared Unix-seconds timestamp conversion. The existing conversion behavior and walk/compare/properties tests remain unchanged.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
