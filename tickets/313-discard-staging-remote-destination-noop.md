# TICKET-313: Fix discard_staging leaving remote stage/backup files behind

Discovered during a full-codebase review (2026-09-22).

## Problem

`discard_staging` in `src-tauri/src/commands/fs/transfer/plan.rs` (around line
419) delegates entirely to `Step::cleanup_unpublished()`
(`recovery/mod.rs:318-360`) whenever `plan.recovery` is `Some` — true on
every macOS run (`recovery::supported() == cfg!(target_os = "macos")`),
regardless of whether the destination is local or remote — and never falls
through to the direct-removal fallback (`dstp.remove(&plan.write_to)`) in
that case.

`cleanup_unpublished()` only deletes an entry when `item.owner` is
`Some(OwnedArtifact { location: RecoveryLocation::Local { .. }, .. })`. But
`Plan::track()` only ever sets `owned_root` (and calls `step.own(...)`) when
`dstp.store_id() == "local"` (`plan.rs:166,192`). For any remote destination
provider (SFTP/FTP/FTPS/SMB/WebDAV), `owner` stays `None` forever, so
`cleanup_unpublished()` is a no-op.

On a Mac build, a safe-mode copy or move to a remote destination that is
cancelled mid-transfer, or fails partway (network blip, remote disk full),
leaves the half-written `.hortcmd-stage-<pid>-<n>` file/directory (and, for a
replace-collision, the `.hortcmd-backup-<pid>-<n>`) permanently on the remote
server — hidden and invisible to the user. Affects the parallel copy path
identically (`parallel.rs:169,262,269` call the same `discard_staging`).

## Required change

- In `discard_staging`, only take the `cleanup_unpublished()` fast path when
  the owner/staging is actually local; otherwise (or as a fallback after
  `cleanup_unpublished()` returns without removing anything) still attempt
  `dstp.remove(&plan.write_to)` when `plan.is_staged() && dstp.exists(&plan.write_to)`,
  as the no-recovery branch already does today.

## Verification

- Extend the protocol harness (`npm run test:harness`) or a mocked-provider
  test to cover: safe-mode copy/move to a remote destination, cancelled or
  failed mid-transfer, macOS recovery path active — assert the remote staging
  entry is removed.
- `npm run check`.
