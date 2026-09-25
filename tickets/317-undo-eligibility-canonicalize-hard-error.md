# TICKET-317: Don't abort a transfer batch when undo eligibility can't be determined

Discovered during a full-codebase review (2026-09-22).

## Problem

`eligibility()` in `src-tauri/src/commands/fs/undo/local.rs` (around line 94)
propagates `parent.canonicalize()?` as a hard `Err` instead of treating a
canonicalize failure as "not eligible for undo," unlike the identical-purpose
`snapshot(source)` call three lines below, which explicitly swallows its
error into `Ok(None)`.

The undo-tracking probe (`Plan::track` → `undo::prepare_group` →
`eligibility`) runs `target.parent().canonicalize()`. If that fails for any
transient/racy reason (an ancestor directory briefly unreadable, removed, or
an OS error between the preceding `parent.is_dir()` check and the
`canonicalize()` call — a real TOCTOU window), the `io::Error` becomes an
`AppError` and bubbles all the way up. `transfer/engine.rs:380-383` treats
any error from `plan.track` as fatal and `break`s the whole per-item loop,
aborting every remaining file in the batch — even though this failure has
nothing to do with the actual copy/move, only with whether an undo record
can be created.

## Required change

- Match on the `canonicalize()` result the same way `snapshot()` is already
  handled two lines below:
  `let Ok(canon) = parent.canonicalize() else { return Ok(None) };
  if !parent.is_dir() || canon != parent { return Ok(None) }`.

## Verification

- Test: simulate a canonicalize failure on the target's parent during a
  multi-file copy/move batch; assert the batch completes (without undo
  tracking for that item) instead of aborting the remaining files.
- `npm run check`.
