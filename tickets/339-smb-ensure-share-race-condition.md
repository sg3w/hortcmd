# TICKET-339: Close the check-then-act race in ensure_share

Discovered during a full-codebase review (2026-09-22).

## Problem

`ensure_share` in `src-tauri/src/vfs/smb.rs` (around lines 152-167) is a
classic check-then-act race: it checks `self.connected` under one lock, drops
the lock, calls `share_connect`, then reacquires the lock to insert — with no
lock held across the whole sequence.

The module's own design explicitly enables concurrent use of one
`SmbProvider` for a remote→remote copy on the same server. Two concurrent
operations against a share that hasn't been attached yet (e.g. copying
between two not-yet-visited folders on the same share, or two panes browsing
the same fresh connection) can both observe `!done.contains(share)` and both
call `self.client.share_connect(...)` for the same tree simultaneously, which
is redundant at best and, depending on the vendored `smb` crate's tolerance
for concurrent tree-connects on one `Client`, could surface a spurious error
on what should be an ordinary operation.

## Required change

- Hold the lock across the whole check-connect-insert sequence (or use an
  entry API / `OnceCell` per share) so only one caller ever issues
  `share_connect` for a given share.

## Verification

- Test: two concurrent operations against the same not-yet-attached SMB
  share; assert only one `share_connect` call happens and neither operation
  surfaces a spurious error.
- `npm run test:harness` (SMB), `npm run check`.
