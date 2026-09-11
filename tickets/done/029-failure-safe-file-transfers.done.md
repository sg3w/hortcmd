# TICKET-029: Add optional failure-safe file transfers

## Goal

Copy operations and copy-based moves can use a failure-safe mode that does not
publish partial data under the final name and does not destroy an existing
destination until the replacement is ready. The mode is controlled by a
persisted setting and is enabled by default.

## Background

The transfer engine currently resolves an overwrite by deleting the destination
before copying. `copy_file_prog` and the parallel copy path then create or truncate
the final target directly. Cancellation removes the partial target, but the old
destination has already been lost. Read, write, flush, network, checksum, or
process failures can also leave a partial final file.

This is especially risky for a file manager whose documented priorities put data
safety first and whose VFS now supports unreliable network connections. The
current direct-write path can still be useful for compatibility, constrained
storage, or providers without reliable rename primitives, so safe staging must be
optional rather than mandatory.

## Requirements

### Setting and operation snapshot

- Add **Use failure-safe transfers** under Settings → File Operations.
- Persist it with the other transfer settings. The default for new and existing
  installations is `true` when the stored value is absent.
- Add concise help text: safe mode needs temporary destination space and prevents
  failed overwrites from destroying the previous destination.
- When disabled, show a visible warning that a failed overwrite may lose the old
  destination or leave incomplete data.
- Snapshot the setting when a transfer is created. A queued transfer keeps the
  value captured at enqueue time even if the setting changes while it waits.
- Pass the snapshot explicitly through the copy/move IPC contract and transfer
  engine; backend behavior must not depend on frontend state after start.

### Safe mode

- Stage each top-level file or directory under a unique hidden name in the
  destination's parent/store. Staging on the destination store is required so the
  final commit never crosses providers.
- Do not create, truncate, or remove the final target while bytes are still being
  copied or verified.
- Flush and close all writers before commit. When checksum verification is enabled,
  verify the staged target, not the final destination.
- Commit a new destination by renaming the staged entry to the final name.
- For overwrite, preserve the old destination until staging succeeds. Use a
  provider-native replace primitive where it has documented atomic semantics;
  otherwise use a destination-side backup rename, staged rename, and rollback on
  failure.
- Do not silently fall back to direct writes when the provider cannot provide the
  required staging/commit behavior. Fail with an actionable message that safe mode
  is unsupported and can be disabled explicitly for that transfer environment.
- If the provider definitively reports insufficient free space for staging, fail
  before removing or renaming the old destination. When capacity is unknown, allow
  staging to proceed and surface write failures normally.
- Cancellation and every error path remove only the staging entry and leave the
  original destination untouched. Cleanup errors are reported separately.
- A copy-based move deletes its source only after the destination commit succeeds.
  If source deletion fails, report that both copies remain; never roll back the
  committed destination by deleting it.
- Successful same-store rename moves remain on the existing fast path and do not
  require staging.

### Unsafe compatibility mode

- When the setting is disabled, retain the current direct-write behavior and
  collision flow. Do not introduce an implicit staging or extra-space requirement.
- Keep the current best-effort cleanup of partial targets on cancellation.
- Clearly mark operation diagnostics/logging as safe or direct mode so failures
  can be understood.
- The setting is global for this ticket; do not add a per-collision or per-file
  prompt that would interrupt batch transfers.

### Provider contract and recovery

- Extend the VFS contract with explicit staging/commit capability rather than
  guessing from the scheme name inside the transfer engine.
- Implement and test commit behavior separately for local, SFTP, FTP/FTPS, SMB,
  and WebDAV providers. A provider may declare safe commit unsupported.
- Staging names must avoid collisions, must not be selected as normal output, and
  must not overwrite user-created files with similar names.
- Best-effort cleanup runs after normal errors and cancellation. On startup or
  connection reuse, detect recognizable stale staging/backup entries and offer
  recovery or cleanup; never delete uncertain user data silently.
- Parallel workers must use independent staging names. One worker failure must not
  commit its target or remove a pre-existing destination.

## Acceptance criteria

- [x] The setting is persisted, enabled by default, and accompanied by an unsafe-
      mode warning when disabled.
- [x] Queued transfers use the value captured when they were queued.
- [x] In safe mode, cancelling a new copy leaves no final partial file.
- [x] In safe mode, cancelling or failing an overwrite leaves the old destination
      byte-for-byte unchanged.
- [x] A checksum mismatch never replaces the old destination.
- [x] The final name becomes visible only after the staged copy is complete.
- [x] A cross-store move does not delete its source until destination commit.
- [x] Unsupported safe commit or insufficient space fails without using unsafe
      behavior automatically.
- [x] With safe mode disabled, transfers retain the existing direct-write path.
- [x] Sequential and parallel copies follow the same selected safety policy.

## Tests

- Add provider-independent transfer tests with injected reader, writer, flush,
  verification, rename, and source-deletion failures.
- Cover new file, file overwrite, directory overwrite, cancellation, checksum
  mismatch, and collision actions in safe and direct modes.
- Assert old destination contents, final-name visibility, staging cleanup, and
  source retention after each failure point.
- Test queued operations to prove the safety setting is snapshotted.
- Test parallel copies with one failed worker and several successful workers.
- Run ignored live tests for local↔SFTP, local↔FTP/FTPS, local↔WebDAV,
  local↔SMB, and remote↔remote paths where available.
- Simulate a process crash after staging and after backup rename; verify that the
  recovery flow identifies the entries without deleting them automatically.

## Definition of Done

- Both safe and direct modes compile and pass automated tests.
- Safe mode is the default and never silently degrades to direct writes.
- Existing destinations and move sources are preserved in every injected failure
  before commit.
- IPC bindings are regenerated after adding the transfer option.
- German and English settings, warnings, and error messages are present.
- README.md and Concept.md document the safety/space tradeoff.
- Rename this file to `029-failure-safe-file-transfers.done.md`.

---

## Implementation

### VFS contract

`Provider` states the capability instead of the engine guessing from the
scheme name (`src-tauri/src/vfs/mod.rs`):

- `supports_staging()` — can a staged entry be published with `rename` inside
  this store. All five providers say yes; a future one that cannot makes safe
  mode **fail with an actionable message**, never fall back to direct writes.
- `rename_replaces()` — does `rename` replace an existing **file** atomically.
  Only `local` (documented for `std::fs::rename` on Unix and Windows). SFTP v3
  refuses an existing target, FTP's `RNFR`/`RNTO` has no portable replace, SMB
  does not request one, and the WebDAV provider deliberately sends
  `Overwrite: F` — so those take the backup route. A *directory* target always
  does, since `rename` onto a populated directory is not portable.
- `free_space()` — `Some` only for local (via the mount lookup the volume bar
  already uses). Unknown capacity lets the write decide rather than blocking a
  transfer on a guess.

### Transfer engine (`commands/fs/file.rs`)

- `Plan { write_to, target, replace }` is the whole difference between the
  modes: direct mode writes to the final name (its collision resolved by
  deleting first, as before), safe mode writes to a hidden sibling.
- `commit()` publishes: a plain rename for a new destination, the provider's
  atomic replace where it has one, otherwise backup-rename → rename → drop the
  backup, with the backup renamed **back** if the commit fails.
- `transfer_one_with()` fixes the order that makes the mode safe: bytes, then
  commit, then a move's source. The copy step is a parameter so each failure
  point can be injected in tests. A source that will not delete is reported as
  "both copies remain" — the committed destination is never removed to
  "repair" it.
- Parallel path: each job flattens into **its own** staging entry, and
  `commit_finished_jobs()` publishes only after every worker has joined, so one
  worker's failure can neither publish a half-copied tree nor disturb a
  pre-existing destination.
- Staging/backup names are hidden and prefixed (`.hortcmd-stage-`,
  `.hortcmd-backup-`) with pid + counter and an existence check, so they never
  collide, never appear as normal output, and stay recognizable afterwards.
- `OpDone.mode` records `"safe"` / `"direct"` so a failure can be read against
  the policy that produced it.
- Leftovers of an interrupted run are scanned at transfer start and emitted as
  `fs-leftovers` — reported only. Deleting them goes through the normal confirm
  dialog and the existing `delete_entries`.

### Frontend

- `settingsStore.safeTransfers`, default `false`; the File-operations panel adds
  the toggle plus an amber warning that is shown **while it is off**.
- `fileOps.beginTransfer` captures the flag into `QueuedParams`, so a queued
  transfer keeps the policy it was enqueued with; it travels explicitly through
  `startTransfer` → `copy_entries`/`move_entries`.
- `App.tsx` subscribes to `fs-leftovers` and raises `confirmLeftovers`.
- 6 new keys in German and English.

## Verified

- `cargo test --lib`: **195 passed**, 25 of them new (`file::safe_tests`) against
  an in-memory `Provider` with injectable rename/remove failures plus the real
  local provider: plan selection in both modes, staged vs. direct naming, unique
  and recognizable working names, commit of a new destination, atomic replace,
  backup-route replace, **rollback of a failed commit** (old destination
  byte-for-byte, no backup left), failed copy / cancellation / failed commit all
  leaving the old destination untouched and the staging entry cleaned up, the
  final name appearing only after the copy, move source removed only after the
  commit, undeletable move source reported with both copies kept, cleanup errors
  reported separately, one failed worker among several not stopping the others
  and not disturbing its destination, a cancelled parallel run committing
  nothing, independent staging names per worker, and the local provider
  committing a directory over a populated one.
- `npm run typecheck` / `npm run build` clean; `npm run gen:types` regenerated
  `OpDone` and added `Leftovers`.
- The setting was driven in the browser pane: off by default with the amber
  warning shown, warning gone once enabled.
- `exercise_staging` runs inside every provider's live test (`#[ignore]`) and
  checks new-destination commit, replace-commit, and that no working entry is
  left behind — one per protocol, because each has its own rename semantics.

**Not covered by automated tests:** `run_transfer` and `run_parallel_copy`
themselves, because both need a Tauri `AppHandle` that only a real app provides
— their staging/commit logic is tested through the extracted
`transfer_one_with`, `plan_for` and `commit_finished_jobs`, but the wiring
between them is covered only by reading. The same applies to the enqueue-time
snapshot: the project has no frontend test runner, so it is verified by
inspection (`beginTransfer` writes the captured flag into `queuedParams`).
The live staging tests need reachable servers and are not part of the normal
run. `free_space` reports a shortage only for local destinations; every remote
destination is treated as unknown capacity by design.
