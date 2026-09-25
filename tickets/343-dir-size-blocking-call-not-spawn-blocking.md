# TICKET-343: Move dir_size's is_dir() check into spawn_blocking

Discovered during a full-codebase review (2026-09-22).

## Problem

`dir_size` (`src-tauri/src/commands/fs/size.rs`, around line 153) is an
`async fn` that calls the blocking `root.is_dir()` (a `stat` syscall)
directly on the async executor, unlike every other blocking op in this
file/`volume.rs`, which is wrapped in `spawn_blocking`.

`RecursiveSize` is a local-only capability, but "local" includes OS-mounted
network shares (e.g. an SMB/AFP volume mounted under `/Volumes/...`). If that
mount is stale/hung (the exact case `volume.rs`'s own comment calls out:
"network shares, spun-down drives"), `is_dir()` blocks the Tokio worker
thread for the syscall's full timeout, stalling other concurrently-scheduled
async IPC commands on that worker and making the whole app appear to hang,
not just the size dialog.

## Required change

- Move the `root.is_dir()` check inside the same `spawn_blocking` closure
  that runs `walk_size` (or merge it into `walk_size`'s first `read_dir`/
  `metadata` call), mirroring the pattern already used in `volume.rs`.

## Verification

- Manual/mocked test: a stale/hung mount point passed to `dir_size`; assert
  other concurrent async commands are not stalled while it's pending.
- `npm run check`.
