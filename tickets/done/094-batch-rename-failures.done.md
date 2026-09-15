# TICKET-094: Preserve files after batch-rename staging or publication failures

## Problem

The two-phase batch rename continues publishing after a source could not be moved
to its temporary name. A later item may then overwrite that unstaged source.
Likewise, rollback after a failed swap can overwrite an already published result.
Found while preparing inverse batch operations for TICKET-048; interposed for data
safety before implementing undo.

## Requirements

- Abort publication when any valid source could not be staged; restore staged
  sources where their original names are still free.
- Never publish onto a target that appeared or remained after staging.
- Never roll back over a path already occupied by another result or an external file.
- Preserve and report the exact temporary path when rollback cannot be completed.
- Keep successful swaps/chains and precise partial-result reporting.

## Acceptance Criteria

- [x] Injected staging failures preserve all original contents.
- [x] Failed publication/rollback never overwrites a successful result.
- [x] A target created during staging and an unchanged source remain untouched.
- [x] Existing rename tests and `npm run check` pass.

## Definition of Done

Commit the tested fix on its own branch, merge locally and rename with `.done.md`.

## Implementation and validation

Publication now stops after a staging failure. Publication and rollback check for
occupied names, and failed rollback reports the retained temporary path. Three
fault-injection regressions failed before the fix and pass afterwards; unchanged
sources and existing successful chains/swaps are covered too. `npm run check` passes.
These provider existence checks do not promise an atomic no-replace primitive
against another process racing between the check and rename.
