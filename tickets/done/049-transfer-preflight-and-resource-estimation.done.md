# TICKET-049: Add transfer preflight and resource estimation

## Goal

Detect predictable transfer failures and explain operation cost before data starts
moving, while keeping final safety decisions authoritative at write and commit
time.

## Background

The transfer engine traverses sources to calculate file and byte totals and then
resolves collisions as work begins. Local volume information exposes free space,
but the VFS has no provider-level capacity, quota, filename, timestamp, or maximum-
object constraints. Users can therefore wait through planning or copying before
discovering insufficient space, unsupported names, read-only destinations,
provider limits, recursive self-copy, or a connection that cannot satisfy the
chosen safety mode.

TICKET-029 already requires safe mode to fail when staging or sufficient space is
unavailable. This ticket supplies a reusable plan and early estimate; it does not
replace commit-time checks or promise that remote capacity remains unchanged.

## Requirements

- Define a typed immutable `OperationPlan` containing normalized sources,
  destination mappings, provider/store identities, item/byte estimates, collision
  summary, required temporary space, safety mode, and known warnings.
- Detect source-equals-destination, destination-inside-source, duplicate targets,
  case-folding conflicts, invalid/reserved names, unsupported metadata, path-length
  limits, read-only roots, and unavailable connections before starting work.
- Add optional provider queries for available capacity, quota, maximum object size,
  maximum path/name length, and staging support, with explicit unknown values.
- Calculate peak space for safe staging, replacement backup, checksum verification,
  archive extraction/packing, and cross-store moves without overflow.
- Distinguish blocking failures, required user decisions, and advisory warnings.
- Show a review summary for unusually large, cross-provider, insecure-transport, or
  irreversible operations; keep ordinary small operations fast.
- Snapshot the approved plan and transfer settings when queued. Revalidate
  connection generation, destination identity, collisions, and capacity immediately
  before writes and commit.
- Make traversal cancellable and bounded; stream or page very large plans rather
  than retaining every file twice in memory.
- Keep plans free of credentials and redact sensitive paths in diagnostics.
- Expose a dry-run API suitable for the future MCP/plugin brokers without granting
  permission to execute the plan.

## Acceptance Criteria

- [x] Predictable self-copy, naming, capability, staging, and known-capacity
      failures are reported before destination content changes.
- [x] Unknown remote quota is represented as unknown, not unlimited.
- [x] Space estimates include peak temporary requirements for the selected mode.
- [x] Queued operations retain the approved immutable plan and settings but still
      revalidate volatile facts before execution.
- [x] Planning remains cancellable and memory-bounded for very large trees.
- [x] A dry run has no filesystem side effects and cannot be executed as authority.
- [x] Commit-time safety remains authoritative when conditions change after review.

## Tests

- Test nested sources, recursive self-copy, duplicate mappings, case collisions,
  reserved names, long paths, sparse files, hard links, and arithmetic overflow.
- Use mock providers with known, unknown, changing, and contradictory quota values.
- Change destination capacity, permissions, contents, and connection generation
  between plan approval and execution and verify safe revalidation.
- Test sequential and parallel plans in safe/direct modes and with checksum on/off.
- Test cancellation and bounded memory with millions of synthetic entries.
- Verify dry runs create no files, provider mutations, credentials, or journaled
  execution authority.

## Definition of Done

- Transfers use a typed, immutable, reviewable operation plan.
- Known invalid operations fail before writes and volatile conditions are rechecked.
- Capacity and provider limits are explicit, including unknown states.
- Planning is cancellable, bounded, and reusable by trusted future interfaces.
- Estimation semantics and limitations are documented.
- Rename this file to `049-transfer-preflight-and-resource-estimation.done.md`.

## Implementation scope and evidence

Implemented typed immutable plans, optional provider limits, streamed bounded
summaries, read-only/cancellable IPC, DE/EN confirmation summaries, queued plan and
settings snapshots, connection generations, before-write source/destination checks
and staged commit collision revalidation. Archive pack/extract capacity checks run
before staging. Unknown constraints are explicit and do not promise unlimited space.

Windows/Linux native constraint queries and their real filesystem matrix are
tracked in [TICKET-120](120-preflight-native-provider-limits.md), per the user's
current scope. Legacy remote directory APIs still return a provider vector before
the planner's bound is checked; the planner does not keep another full file tree.
The exact memory/selection bounds, conservative case handling and space semantics
are documented in [Transfer preflight](../../docs/transfer-preflight.md).

Tests cover overlap/duplicates, Unicode normalization, declared name/object bounds,
checked space arithmetic, capacity/quota changes, source/destination replacement,
connection generations, synthetic million-entry bounds, cancellation and refusal
of a target changed before commit. UI tests cover blocked confirmation, cancellation
and immutable settings. `npm run check` validates the complete implementation.
