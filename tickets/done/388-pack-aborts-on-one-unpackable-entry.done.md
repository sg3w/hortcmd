# TICKET-388: Keep ZIP packing alive when a single entry cannot be packed

Priority: **P1**. Status: **open**.
Reported from use on 2026-09-29: packing a real folder on a local SSD fails
outright with "unsafe entry", and nothing is produced.

## Problem

`pack::run_with_recovery` treats every entry it cannot pack as a reason to
abandon the whole archive. On a synthetic fixture that is defensible; on a
real home directory it makes the ZIP command unusable, because ordinary
content trips it:

- a symlink anywhere below a source root — `~/Library`, a `node_modules`
  `.bin`, a Homebrew tree, an alias the user made years ago
  (`archive/pack.rs:70-73` for the root, `:138-144` for the second pass),
- a socket, FIFO or device node (`!directory && !meta.is_file()`, `:104`),
- a name `safety::path` refuses: a trailing dot or space, a colon — all of
  which macOS and Linux create happily — or a Windows device stem
  (`archive/safety.rs:79-110`),
- a name that is not valid UTF-8 (`:100-103`, `into_string`),
- a subdirectory `read_dir` cannot open, or an entry whose metadata cannot be
  read: both are a bare `?` (`:98`, `:131`),
- a file that changed between the collection pass and the write pass — a
  different size, or a file that became a directory (`:139-144`), which is
  simply what a live filesystem does during a long pack,
- a name collision the manifest detects (`safety::conflict`).

Each of these returns from the closure, the staging file is discarded, and
the user is told "unsafe entry" for an archive of ten thousand files in which
one was a symlink. There is no way to find out which entry it was, and no way
to get the other 9999.

The project already decided the opposite for the neighbouring walkers.
Ticket 326 made a failed `metadata()` per entry non-fatal, ticket 384 did the
same for an unreadable subdirectory in compare, and `fs/walk.rs:19` counts
unreadable directories rather than failing. Extraction reports per-item
failures through `ItemError` and still finishes
(`archive.rs:320-333`, `extract::run` returns `(u32, Vec<ItemError>)`).
Packing is the one operation left that is all-or-nothing, and it returns a
bare `u32` so it has nowhere to put a partial result.

Limits are a different matter and stay fatal: `entries`, `total_bytes`,
`entry_bytes`, `depth`, `path_bytes` and `ratio` bound what the process will
do, and skipping past them would defeat them. So does a destination that
overlaps a source, which is a request that cannot be satisfied at all.

## Required change

- `pack::run` and `pack::run_with_recovery` return
  `(u32, Vec<ItemError>)`, matching `extract::run`. The GUI adapter
  `run_pack` passes those errors into `OpDone.errors` the way `run_extract`
  already does, so the transfer summary names each skipped path.
- Every per-entry refusal above — unreadable metadata, symlink, non-regular
  file, undecodable name, a name `safety::path` or the manifest refuses, an
  unopenable directory, an entry that changed between the two passes — skips
  that entry and continues. A skipped directory is not descended into; a
  skipped file is not written.
- A file whose read fails mid-stream, or whose length no longer matches, is
  removed from the archive with `ZipWriter::abort_file` rather than left
  truncated under its name, and is reported as a skip.
- Limit errors, the overlapping destination, a failure on the destination
  itself, and cancellation stay fatal exactly as they are.
- All sources unusable is a failure, not an empty archive: the first recorded
  error is returned and nothing is published.
- The recorded skip list is bounded so a directory of a million refused names
  cannot grow it without limit; the count of skipped entries is reported
  regardless.

## Verification

- Rust test: a source tree holding a symlink, a FIFO and a regular file packs
  the regular file, reports two skips, and produces a readable ZIP.
- Rust test: an entry whose size changes between the two passes is left out
  of the archive rather than aborting it, and the ZIP stays readable.
- Rust test: an unreadable subdirectory is skipped and its siblings are packed.
- Rust test: limits, the overlapping destination and cancellation still fail
  the whole operation and leave a previous archive at the destination intact.
- Rust test: a source list in which nothing is packable fails and publishes
  nothing.
- `npm run check`.

## Implementation

`pack::run` and `run_with_recovery` now return `(u32, Vec<ItemError>)`, the
same shape `extract::run` has always had, and a `Skips` collector decides what
goes in the second half.

- `fatal(&AppError)` is the whole policy in one function: `ARCHIVE_LIMIT` and
  `CANCELLED` end the archive, everything else ends one entry. The limits are
  the bound the process promised to stay inside and a cancel is the user asking
  to stop; skipping past either would defeat it.
- Authorization stays fatal by construction rather than by error code. Every
  `check(&path)` is called with `?` outside the skippable region — once per
  entry while collecting, once per entry before writing, once per buffer inside
  a file — so a revoked MCP write permission or a scope change still aborts
  before anything is published, instead of quietly producing an archive with
  the out-of-scope half missing.
- The collection pass treats roots and descendants alike: unreadable metadata,
  a symlink, a socket/FIFO/device, an undecodable name, and a name the manifest
  refuses (`safety::path` or a spelling/casing conflict) are each one skip. An
  unreadable directory is still recorded as an entry, so the archive says the
  folder was there and empty rather than saying nothing about it.
- `pack_one` holds the write pass. Its `Outcome::Skipped` is an entry that
  changed under the packer — a different size, a different kind, a file that
  became a link, a read that failed mid-stream — and every one of those calls
  `ZipWriter::abort_file` first, so no truncated member is left under a name
  that promises a whole file. `Err` from it is still fatal.
- Nothing packable is a failure, not an empty archive: the collection pass
  returns the first recorded reason before staging exists, so the destination is
  never touched. An overlapping destination stays fatal for the same reason —
  it is a request that cannot be satisfied by leaving entries out.
- `MAX_REPORTED_SKIPS` caps the named entries at 100 while `Skips::total` keeps
  counting, so a directory of a million refused names cannot grow the payload.
- The GUI adapter `run_pack` passes the list straight into `OpDone.errors`, the
  way `run_extract` already did, so the transfer summary names each skipped
  path. The MCP tool keeps the stricter contract and fails when the list is not
  empty — an MCP caller named these sources and is told when one was left out.

### Verification

- `packing_skips_links_and_special_files_and_still_finishes`: a source holding
  a directory symlink that would recurse, a FIFO and one regular file packs the
  file, reports two `archive.unsafeEntry` skips, and produces a ZIP that has
  neither of the two.
- `packing_skips_a_source_that_changed_between_the_passes`: a file grown on its
  second `check` is absent from the finished archive, reported once as `fs.io`,
  and its sibling is packed — the ZIP holds exactly two members and reads back.
- `packing_skips_an_unreadable_directory_and_packs_its_siblings`: a `0o000`
  subdirectory is one `fs.permission` skip, is present in the archive as an
  empty folder, and its sibling file is packed.
- `duplicate_packed_names_are_skipped_once_and_reported`: the same source twice
  produces a one-member archive and one `archive.conflict` skip, replacing the
  old assertion that the whole operation failed.
- `packing_nothing_usable_fails_without_touching_the_destination`: a single
  source root that is a symlink fails with `archive.unsafeEntry`, the previous
  archive at the destination is byte-identical, and no staging entry is left.
- `cancelling_inside_a_packed_file_preserves_the_previous_archive` and
  `oversized_source_list_is_rejected_before_path_resolution` are unchanged and
  still pass, which is what holds the two fatal cases in place.
- `npm run check` passed on macOS (arm64).
