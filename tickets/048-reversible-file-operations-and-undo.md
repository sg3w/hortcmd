# TICKET-048: Add reversible file operations and bounded undo

## Goal

Allow users to reverse eligible rename, move, trash, copy, create-directory, and
metadata operations without implying that every destructive action is recoverable.

## Background

The application supports collision handling, the operating-system trash, copy and
move progress, batch rename, synchronization, permissions, ownership, and tags.
The roadmap lists undo as unfinished, but there is no operation-history contract
that records the information needed to reverse completed work. A generic “Undo”
label would be dangerous unless reversibility, conflicts, remote-provider support,
and expiration are explicit.

This ticket builds on failure-safe commits from TICKET-029, provider capabilities
from TICKET-031, durable operation records from TICKET-044, and structured errors
from TICKET-036.

## Requirements

- Define an `UndoPlan` created before commit for each supported operation, including
  inverse steps, affected provider/store, stable identities where available,
  expected post-operation metadata, and expiration conditions.
- Classify operations as fully reversible, conditionally reversible, or
  irreversible before confirmation and display that classification to the user.
- Support at minimum:
  - rename and same-store move by inverse rename;
  - copy by deleting only destinations still matching the committed result;
  - new directory by removing it only if still empty and unchanged;
  - trash by platform/provider restore when a stable restore token exists;
  - permissions, owner, and tags by restoring captured prior metadata.
- Treat permanent delete, overwritten content without a retained safe backup, and
  providers lacking required primitives as irreversible.
- Never undo by overwriting a path that changed after the original operation.
- Resolve inverse-operation collisions through a dedicated preview; do not reuse an
  “overwrite all” decision without showing undo-specific consequences.
- Group multi-item and batch operations atomically where possible and report
  partial undo precisely when a provider cannot guarantee atomicity.
- Persist a bounded, privacy-conscious undo history without credentials or file
  contents. Expire entries when providers disconnect permanently, backups are
  removed, or required state no longer matches.
- Add keyboard and menu entry points that show the exact next reversible operation.
  Integrate into **Dateien** through the shared command definition from
  [TICKET-040](040-native-application-menu.md), once file undo is implemented.
  Native macOS Edit-menu undo remains text editing; route shortcuts by focus and
  never undo a file operation while editing text. TICKET-040 ships no file-undo
  placeholder in advance of this feature.
- Permit redo only where it can be represented as a newly validated operation,
  not as blind replay of stale paths.

## Acceptance Criteria

- [ ] The UI identifies whether an operation is reversible before execution.
- [ ] Undo never modifies an item that no longer matches the recorded result.
- [ ] Rename, eligible move/copy, empty-directory creation, and metadata changes
      have tested inverse plans.
- [ ] Permanent and unsupported actions never create misleading undo entries.
- [ ] Multi-item partial failures identify exactly what was and was not restored.
- [ ] Undo history survives restart according to a documented bounded retention
      policy and contains no secrets.
- [ ] Remote support follows provider capabilities and reconnect trust rules.

## Tests

- Test every reversible operation locally and on each capable remote provider.
- Modify, replace, rename, or delete affected paths between operation and undo and
  verify the inverse fails safely with an actionable preview.
- Test collisions, case-only renames, cross-store moves, partial operations,
  disconnect/reconnect, restart, history expiration, and concurrent external edits.
- Verify copy undo deletes only byte/identity-matching committed outputs.
- Verify grouped undo behavior with failures in the middle of an inverse plan.
- Scan persisted history and diagnostics for file content and credential canaries.

## Definition of Done

- Reversibility is an explicit capability and per-operation contract.
- Supported operations create validated, bounded undo plans and expose clear UI.
- Irreversible actions are labeled honestly before confirmation.
- Concurrent changes and collisions cannot be overwritten by undo.
- Undo behavior is durable, tested across providers, and documented.
- Rename this file to `048-reversible-file-operations-and-undo.done.md`.
