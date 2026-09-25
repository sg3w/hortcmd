# TICKET-315: Clean up orphaned directory skeletons on parallel-copy abort

Discovered during a full-codebase review (2026-09-22).

## Problem

`run_parallel_copy` in `src-tauri/src/commands/fs/transfer/parallel.rs`
(around lines 158-172) flattens every job's tree up front, creating
destination directories as it goes (`dstp.create_dir_all` inside
`flatten_tree`). If flattening job *i* fails, the code calls
`discard_staging` for every job, including ones that already flattened
successfully.

In safe mode this correctly removes each job's staging entry, but in
**direct mode** (`safe=false`, or a non-staging destination) `plan.write_to
== plan.target`, so `discard_staging` is a no-op
(`!plan.is_staged()` short-circuits at `plan.rs:426-428`) — the already
`create_dir_all`'d destination directory trees for the *not-yet-failed* jobs
are left sitting in the user's actual destination with no files in them,
even though the command reports nothing succeeded.

## Required change

- Track which directories `flatten_tree` created per job and remove them
  (best-effort) on this all-or-nothing abort path in direct mode, or make the
  abort behavior explicit about which jobs get partial directory skeletons.

## Verification

- Test: parallel-copy several folders in direct mode where one folder's
  flatten fails (permission-denied subdirectory); assert no empty directory
  skeletons remain at the destination for the other jobs.
- `npm run check`.
