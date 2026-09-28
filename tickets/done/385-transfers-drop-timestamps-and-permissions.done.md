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

## Implementation

The `Provider` trait gains a metadata pair, both optional:

- `read_metadata` returns the `FileMetadata` a copy should inherit — the
  modification time and, where the store has one, the Unix permission bits.
  The default reports `None`, which is the honest answer for every adapter
  whose protocol has no such notion.
- `apply_metadata` writes it to an entry the transfer has just produced. The
  default is `ErrorKind::Unsupported`, so a store that cannot set a timestamp
  says so instead of silently succeeding.

Reading is a second method rather than a field on `Stat` because `Stat` is the
hot planning call: every provider fills it for every entry, and none of them
would have anything to put in the new fields. The one operation that "defaults
to unsupported" — the one the ticket asks for — is `apply_metadata`.

`LocalProvider` implements both. Applying sets the timestamp first and the mode
second, because a restored read-only mode would otherwise refuse the handle the
timestamp needs; a directory is opened read-only, which is what `futimens`
wants, and on a platform without that guarantee its timestamp is left alone
rather than failing the transfer.

`preserve_metadata` (in `copy.rs`) joins the two halves and is called from both
paths after the bytes are committed: `copy_file_prog` and `parallel_copy_file`
for files, `copy_tree` after a directory's children (writing them is what last
changed the directory's own timestamp), and `run_parallel_copy` for the
flattened tree's directories once every worker has joined — deepest first, for
the same reason. In safe mode this all happens under the staging name, so the
publishing rename carries the metadata with the bytes.

A metadata failure never fails a transfer whose bytes are correct. `Prog` and
`SharedProg` carry a shared `Notes` list; a refusal is recorded there against
the item, and the engine drains it into the operation's error list at the end,
with `op = "preserve-metadata"` and `ErrorAction::Report`. A destination that
answers `Unsupported` is not an error at all — it promised nothing, and the
plan said so beforehand.

`LocalProvider::limits` now answers `preserves_metadata: Some(true)`, so the
`metadataUnknown` advisory stops firing on every local plan. To keep it
truthful, the preflight raises it per item for a cross-store source even when
the destination is local: the destination's fidelity says nothing about what
the source can report.

### What a transfer promises

Written into the module comment of `copy.rs`. In short: a same-store `rename`
keeps the object and therefore everything about it; every other path is a
streamed copy that reproduces mtime and, on Unix, the mode — and nothing else.
Extended attributes, ACLs, ownership and creation times are not carried on any
path. A move across devices is a copy and follows the copy rule, rather than
silently differing from the rename it falls back from. A cross-provider copy
carries whatever both ends can name, which today is nothing, and keeps its
advisory.

### Verification

- `transfer::tests::a_copy_keeps_the_timestamp_and_the_mode`: a tree with a
  0755 script and a 0600 file, both stamped at 2001-09-09, is copied through
  `copy_tree` and through `parallel_copy_file`. Both targets carry the exact
  mtime and mode, the copied directory carries its source's timestamp, and no
  note was recorded.
- `a_refused_metadata_step_is_a_note_not_a_failure`: a destination that
  implements no metadata operation copies cleanly with no remark, and a real
  refusal produces a `preserve-metadata` item error rather than a failed copy.
- `preflight::tests::metadata_advisory_fires_only_where_fidelity_is_unknown`: a
  local plan carries no `metadataUnknown`; `LocalProvider::limits` answers
  `Some(true)`; a provider on the trait defaults still answers `None`, so its
  plans keep the advisory.
- `compare::paired_safety_tests::a_copied_file_compares_equal_to_its_source`:
  a file copied by the engine classifies as `same` against its source through
  `compare_levels_impl` — the regression this ticket is really about.
- `npm run check` passed on macOS (arm64).
