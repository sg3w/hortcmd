# TICKET-350: Use the dedicated unknown-format code in list_archive

Discovered during a full-codebase review (2026-09-22).

## Problem

Unknown archive format in `read_archive_inner`
(`src-tauri/src/commands/fs/archive.rs`, around line 273, used by
`list_archive`) is reported via a raw string turned into
`code::INTERNAL_UNEXPECTED` + `ErrorAction::Report`, instead of the dedicated
`code::ARCHIVE_UNKNOWN_FORMAT` that `extract::run` already uses for the
identical condition (`extract.rs:453`).

A user tries to browse a `.rar` (or any unsupported) archive from the file
list; the frontend gets a generic "internal error, please report" action
instead of the structured unsupported-format code it would get from an
equivalent extract attempt, diverging behavior between two callers of the
same `archive_kind()` detector.

## Required change

- Replace `.ok_or_else(|| "Unbekanntes Archivformat".to_string())?` at
  `archive.rs:273` with
  `.ok_or_else(|| AppError::new(code::ARCHIVE_UNKNOWN_FORMAT))?`.

## Verification

- Test: browse an unsupported archive format via `list_archive`; assert the
  same `ARCHIVE_UNKNOWN_FORMAT` code as `extract` produces.
- `npm run check`.
