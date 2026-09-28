# TICKET-385: Preserve timestamps and permissions on copy, and report the fidelity

Priority: **P2**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

Copying a file with hortcmd produces a file with today's date and the
process's default permissions. The source's modification time, its mode bits
and its executable flag are all lost.

The copy path writes bytes and nothing else. `copy_file_prog`
(`src-tauri/src/commands/fs/transfer/copy.rs:40-138`) opens
`dstp.create_write(dst)`, streams blocks, flushes, optionally verifies the
SHA-256, and returns. `copy_tree` (`copy.rs:160-181`) only adds
`create_dir_all`. The parallel path is the same
(`transfer/parallel.rs:119-188`). Nothing calls anything resembling
`set_times` or `set_permissions` — and it could not, because the `Provider`
trait has no such operation at all (`src-tauri/src/vfs/mod.rs:189-299`:
`stat`, `open_read`, `create_write`, `write`, `create_dir_all`, `rename`,
`remove_*`, and no metadata writer).

This is visible everywhere the app shows a date. The comparison feature is
built on "size + modification time" (`compare.rs:88-105`, `classify_pair` at
`compare.rs:341-368`), so a directory that hortcmd copied never compares
`same` against its own source — `newer_left`/`newer_right` for every file.
Directory synchronisation on a tree hortcmd itself produced therefore wants
to re-copy everything. On Unix the lost mode also means a copied script is no
longer executable; `DirEntry.executable` already exists in the listing model,
so the panel will show the difference.

The planning layer already models this and is left permanently uncertain.
`ProviderLimits::preserves_metadata` is documented as "None means
timestamp/permission/extended-attribute fidelity is unknown"
(`src-tauri/src/vfs/limits.rs:22-23`), and the preflight raises an advisory
whenever it is not `Some(true)` (`transfer/preflight/mod.rs:192-194`):

```rust
if plan.limits.preserves_metadata != Some(true) {
    issue(&mut plan, "advisory", "metadataUnknown", None);
}
```

`LocalProvider::limits` never sets the field (`src-tauri/src/vfs/local.rs:158-193`),
so **every** plan — including a plain local→local copy on one disk — carries
`metadataUnknown`. An advisory that is always present is an advisory nobody
reads, which also devalues it for the remote providers where it is genuinely
informative.

## Required change

- Add one optional metadata operation to `Provider` — applying a source
  `Stat`'s timestamp and mode to a freshly written target — defaulting to
  "unsupported" so the remote providers are unaffected until they implement
  it. Implement it for `LocalProvider` (mtime always; mode on Unix).
- Call it from the sequential and the parallel copy after the bytes are
  committed, for files and for directories. A failure to apply metadata must
  not fail the transfer: the bytes are correct, so it belongs in the item's
  error list, not in the result.
- Make `LocalProvider::limits` answer `preserves_metadata: Some(true)` once it
  does, so `metadataUnknown` stops firing on every local plan and starts
  meaning something.
- Decide explicitly, and write down in the module comment, what a *move*
  across devices and a *cross-provider* copy promise. `rename` keeps metadata
  for free; a streamed copy does not, and the two must not differ silently.

## Verification

- Rust test: copy a file with a known mtime and mode; assert both survive on
  the target, for the sequential and the parallel path.
- Rust test: a local preflight plan no longer carries the `metadataUnknown`
  advisory; a remote one still does.
- Rust test: `compare_levels_impl` classifies a copied file as `same` against
  its source.
- `npm run check`.
