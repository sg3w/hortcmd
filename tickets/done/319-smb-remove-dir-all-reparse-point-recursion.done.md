# TICKET-319: Don't recurse into SMB reparse points when deleting

Discovered during a full-codebase review (2026-09-22).

## Problem

`dir_entries` in `src-tauri/src/vfs/smb.rs` (around lines 273-290) classifies
`is_dir` purely from `file_attributes.directory()`, ignoring
`file_attributes.reparse_point()` (used only for the display-only
`is_symlink` field). `remove_dir_all` (around lines 504-519) then recurses on
every child where `is_dir()` is true.

A share containing a directory reparse point/junction that points back at an
ancestor (`a/link -> a`), created either by a malicious/compromised server or
by any writer with access to the share, is followed transparently: the
vendored `smb` crate has no reparse-point awareness (no `create_file`/
directory-open guard). Deleting the ancestor directory drives
`remove_dir_all` into `read_dir(a) -> remove_dir_all(a/link) -> read_dir(a)
-> ...`, an unbounded recursion that stack-overflows and crashes the client
on an otherwise ordinary delete.

SFTP already guards against exactly this class of bug: `sftp_metadata::kind()`
reports a symlink-to-directory as `EntryKind::Symlink`, not `Directory`, from
the unresolved `READDIR` attributes (ticket 304), so `sftp.rs`'s
`remove_dir_all_async` never recurses into a link. SMB has no equivalent
guard.

## Required change

- Exclude `info.file_attributes.reparse_point()` entries from the
  `is_dir`-driven recursion in `remove_dir_all` (and any other client-side
  directory traversal), treating them like SFTP treats a symlink:
  delete-as-file, never descend.

## Verification

- Extend the protocol harness's SMB suite with a self-referential junction
  fixture (or a mocked-provider unit test where a real SMB server isn't
  practical) and assert delete does not recurse into it.
- `npm run test:harness` (SMB), `npm run check`.

## Resolution (2026-09-27)

`remove_dir_all` no longer asks `stat` whether a child is a directory — `stat`
resolves the reparse point, which is exactly what makes `a/link -> a` recurse
forever. It now walks `dir_entries` and classifies each child from its
link-level attributes via the pure `child_action` helper: a plain directory is
descended into, a plain file is unlinked, and any entry carrying
`reparse_point()` is deleted as the link itself through
`remove_reparse_point`, which opens it with `FILE_OPEN_REPARSE_POINT |
DELETE_ON_CLOSE` so the server never follows it and the target stays
untouched. This mirrors how SFTP's `remove_dir_all_async` treats a
symlink-to-directory. Deleting the connection root (which lists shares, not
directory entries) now fails with a clear message instead of trying to delete
shares.

Unit tests over `child_action` assert that a directory reparse point maps to
`DeleteLink`, never `Descend`, and that plain entries keep their previous
handling. A real self-referential junction needs an SMB server, so the
protocol-harness fixture stays for the native SMB verification tickets.

Validation: `npm run check` passed on macOS.
