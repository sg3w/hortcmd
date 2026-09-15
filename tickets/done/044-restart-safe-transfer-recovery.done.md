# TICKET-044: Recover transfer state safely after interruption or restart

## Goal

Ensure queued and active file operations have an explainable, recoverable state
after an application crash, forced termination, operating-system restart, or lost
frontend event stream.

## Background

The frontend transfer list and queued parameters are in memory. Backend transfer
controls are also process-local, and progress is delivered through transient
events. The close guard helps during an orderly window close, but a crash or power
loss discards queue metadata and can leave users unable to tell which items
completed. TICKET-029 makes individual copies failure-safe through staging and
commit; it does not by itself provide a durable operation journal or restart UI.

Recovery must never guess that a move completed, delete a source based on stale
state, or expose credentials in persisted operation data.

## Requirements

- Define a versioned, crash-consistent operation journal owned by the backend for
  copy, copy-based move, extract, and pack operations.
- Persist operation ID, kind, sanitized source/destination references, item plan,
  committed-item state, staging references, snapshotted transfer settings, queue
  order, timestamps, and a redacted terminal reason.
- Never persist passwords, passphrases, private keys, bearer tokens, credential
  values, or signed URLs. Refer to saved profiles by stable non-secret ID.
- Write journal transitions atomically and flush at commit-critical boundaries.
- On startup, reconcile journal state with destinations and safe-staging artifacts
  before offering actions.
- Distinguish resumable, retryable-from-start, completed-but-unreported, cleanup-
  required, unsafe-direct-write-interrupted, and unrecoverable operations.
- Resume at a verified item/chunk boundary only when both providers and the safe
  transfer mode can prove correctness; otherwise offer retry or cleanup explicitly.
- Preserve queue order and each operation's snapshotted settings across restart.
- Require reconnection/reauthentication through the normal credential flow before
  accessing remote paths; do not weaken host-key or TLS verification.
- Delete a move source only after the destination commit is durably recorded.
- Provide clear UI actions for resume, retry, discard record, clean staging, and
  reveal affected paths, with confirmation for destructive cleanup.
- Apply a bounded retention policy to completed history and journal artifacts.

## Acceptance Criteria

- [x] A forced termination at any tested transfer phase produces a deterministic
      recovery state on next launch.
- [x] No recovery path deletes an uncommitted move source or replaces a valid
      destination with unverified data.
- [x] Queued operations retain order and snapshotted settings after restart.
- [x] Direct-write interruptions are labeled unsafe and are never represented as
      successfully committed.
- [x] Remote recovery waits for a verified session and handles unavailable profiles.
- [x] The journal and support diagnostics contain no credentials.
- [x] Completed records and orphan staging artifacts are cleaned by documented,
      bounded retention rules.

## Tests

- Add deterministic fault injection after planning, staging creation, each write,
  flush, checksum, destination swap, commit record, and move-source deletion.
- Kill and restart the process at every fault point for sequential and parallel
  local/remote transfers.
- Test missing, changed, and temporarily unavailable sources, destinations,
  profiles, and staging artifacts.
- Test migration from every journal schema version and corrupt/truncated records.
- Test safe and direct-write modes, checksum on/off, queued operations, extract,
  pack, and copy-based moves.
- Scan journal files and exported diagnostics for seeded secret values.

## Definition of Done

- Transfer intent and commit state survive abnormal termination.
- Startup reconciliation is deterministic, bounded, and user-visible.
- Recovery preserves the safety guarantees of TICKET-029 and the connection trust
  guarantees of TICKET-028 and TICKET-032.
- Fault-injection tests cover every commit-critical transition.
- Journal schema, retention, and recovery choices are documented.
- Rename this file to `044-restart-safe-transfer-recovery.done.md`.

## Implementation evidence (macOS scope)

The current user instruction defers Windows/Linux work. The adapter is therefore
activated on macOS; native enablement and platform/remote fault-harness runs are
tracked in [TICKET-116](../116-windows-linux-transfer-recovery.md). The journal covers
the desktop transfer queue and desktop archive commands. MCP has its separate
execution lifecycle and is not attached to the desktop queue.

Implemented version-1 atomic, exclusively locked snapshots, bounded retention,
profile-ID references without credentials, staged commit records and conservative
restart reconciliation. Recovery actions require a fresh backend check; neither
startup nor cleanup deletes a move source. Remote item-boundary resumption is
refused when provider durability cannot be proven.

Local process-exit tests exercise sequential/parallel copy, copy-based/native
move, ZIP pack/extract and journal/publication boundaries. Additional tests cover
queue settings/order, direct-write classification, corrupt/future/oversized
snapshots, missing sources/profiles, replaced staging, preserved backups,
cancellation, journal capacity and acknowledgment after a lost terminal event.
Frontend tests cover explicit cleanup confirmation, unavailable actions, queue
order, reattachment and a lost completion event. No live remote endpoint or native
Windows/Linux fault run is claimed here.

Schema, retention bounds and user actions are documented in
[transfer recovery](../../docs/transfer-recovery.md). TICKET-092 was interposed and
completed separately because parallel planning previously swallowed provider
errors, which could allow an incomplete staged tree to be published.

Validation: `npm run check` passed (411 Rust tests, 25 ignored protocol/keychain tests; 522 frontend tests; all remaining checks and production build passed).
