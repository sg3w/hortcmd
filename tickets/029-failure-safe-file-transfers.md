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

- [ ] The setting is persisted, enabled by default, and accompanied by an unsafe-
      mode warning when disabled.
- [ ] Queued transfers use the value captured when they were queued.
- [ ] In safe mode, cancelling a new copy leaves no final partial file.
- [ ] In safe mode, cancelling or failing an overwrite leaves the old destination
      byte-for-byte unchanged.
- [ ] A checksum mismatch never replaces the old destination.
- [ ] The final name becomes visible only after the staged copy is complete.
- [ ] A cross-store move does not delete its source until destination commit.
- [ ] Unsupported safe commit or insufficient space fails without using unsafe
      behavior automatically.
- [ ] With safe mode disabled, transfers retain the existing direct-write path.
- [ ] Sequential and parallel copies follow the same selected safety policy.

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
