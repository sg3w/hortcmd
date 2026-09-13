# TICKET-254: Validate file undo on Windows and Linux

## Goal

Add native Windows/Linux adapters to the file-undo contract introduced by
[TICKET-048](048-reversible-file-operations-and-undo.done.md). Deferred because the
current implementation batch explicitly excludes Windows/Linux work.

## Requirements

- Implement stable identity and atomic no-replace inverse operations for each OS.
- Implement trash restore only where the provider exposes a stable restore token.
- Keep unsupported operations explicitly irreversible until the adapter passes.
- Test filesystem replacement races, metadata, case-only renames, grouped failures,
  restart, retention and text-editing shortcut isolation in native environments.

## Acceptance Criteria

- [ ] Windows adapter passes the contract and native integration tests.
- [ ] Linux adapter passes the contract and native integration tests.
- [ ] Unsupported providers remain fail-closed and documentation is updated.

## Definition of Done

Commit on a dedicated branch after `npm run check` and the native matrix pass.
