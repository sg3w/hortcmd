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
