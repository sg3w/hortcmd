# TICKET-318: Count unreadable files hit during content/duplicate search

Discovered during a full-codebase review (2026-09-22).

## Problem

`content_match` (lines 308-325) and `hash_file` (lines 365-379) in
`src-tauri/src/commands/fs/search.rs` use `.ok()?` to swallow any I/O error
(permission change, deletion, device error) and simply return `None`, with no
increment of any unreadable counter. The file's own doc comment (lines 22-23)
states the contract: "Unreadable entries are counted and reported
('partial')" — but `SearchSummary.unreadable` only reflects
`WalkReport.unreadable` from the stat-time walk, not later read failures.

This is worst in duplicate mode: candidates are collected during the walk,
then hashed in a *second pass* (`find_duplicates`, well after the walk
finished) — a real TOCTOU window where a file can vanish or become unreadable
between listing and hashing and simply disappears from the results with no
trace, while `SearchStatus` can still report `Complete`.

## Required change

- Thread an unreadable counter into `content_match`/`hash_file` (or their
  callers) and fold it into `report.unreadable` so `SearchStatus::Partial` is
  reported whenever a match candidate could not actually be read.

## Verification

- Test: run a duplicate search over a tree where one file of a would-be
  duplicate pair is deleted between the walk phase and the hash phase; assert
  the summary reports `Partial` with a non-zero unreadable count instead of
  `Complete`.
- `npm run check`.

## Resolution (2026-09-25)

Content and duplicate reads now preserve I/O errors and count them in the search summary, producing Partial results. Regression tests cover a vanished duplicate candidate, unreadable content, and binary/nonmatching files.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
