# TICKET-347: Use a specific error code for dir_size's "not a directory" case

Discovered during a full-codebase review (2026-09-22).

## Problem

The "not a directory" case in `src-tauri/src/commands/fs/size.rs` (around
line 158) uses `AppError::from(format!(...))`, which per `errors/mod.rs`
always maps to `code::INTERNAL_UNEXPECTED` + `ErrorAction::Report` — the
generic "unexpected error, please report" UX — for what is actually a common,
recoverable condition (e.g. the folder was deleted or replaced between the
user opening the properties dialog and the async command running).

A user right-clicks a folder for "Properties" and, in the moment before the
IPC call resolves, the folder is deleted/renamed/replaced by another process
(or an unrelated file is passed by mistake); the user sees a generic
"internal error, please report this" dialog instead of a sensible "this
folder no longer exists" message.

## Required change

- Use a dedicated, meaningful code (e.g. `code::FS_NOT_FOUND` or a new
  `fs.notADirectory`) with an appropriate `ErrorAction` instead of the
  generic migration-path `AppError::from(String)`.

## Verification

- Test: call `dir_size` on a path that's been deleted/replaced; assert the
  surfaced error uses the specific code, not the generic internal-error
  fallback.
- `npm run check`.

## Resolution (2026-09-25)

Folder-size validation now preserves filesystem metadata errors and reports files as fs.notADirectory. Missing folders use fs.notFound; neither common condition suggests reporting an internal error. A regression deletes a folder and then replaces it with a file to verify both outcomes.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
