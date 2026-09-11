# TICKET-007: Directory synchronization should not start recursive by default

## Goal
The directory synchronization should never run recursively automatically at startup.

## Background
Currently the synchronization of directories can start directly with recursive processing enabled. As a result, subdirectories are unintentionally included even though the user did not explicitly choose this.

Recursive synchronization should only happen after a deliberate activation by the user.

## Requirements
- The **"Recursive"** option is always disabled when the synchronization dialog is opened.
- A previously used recursive state must not be adopted automatically.
- Synchronization starts by default only for the currently selected directory.
- Subdirectories are only included when the user explicitly enables the **"Recursive"** option.
- The chosen state must be clearly visible before synchronization starts.

## Acceptance criteria
- [x] The synchronization dialog always starts with the **"Recursive"** option disabled.
- [x] Without user interaction, no subdirectories are synchronized.
- [x] A previous recursive synchronization does not affect the next start.
- [x] After manual activation, subdirectories are included correctly.
- [x] The current recursive state is clearly recognizable in the dialog.

## Tests
- [x] Open the synchronization dialog and check the default state. **Verified in
      browser demo mode**: the "Recursive (subfolders)" checkbox unchecked, 5 entries
      (not recursive).
- [x] Start synchronization without enabling **"Recursive"**.
- [x] Check that no subdirectories are processed.
- [x] Enable **"Recursive"** and start synchronization again. **Verified**:
      after enabling, 1507 instead of 5 entries (the `src/` subfolder included).
- [x] Close the dialog and reopen it.
- [x] Check that **"Recursive"** is disabled again. **Verified**: after
      close/reopen unchecked again, 5 entries (the state is not
      adopted).

## Definition of Done
- [x] Code compiles.
- [x] `npm run typecheck`, `npm run build` succeed.
- [x] Existing functionality unchanged.
- [x] Documentation updated (Concept.md).
- [x] Ticket renamed to `*.done.md`.

## Implementation note
Cause in `src/components/ui/CompareDialog.tsx`: both the initial
`useState(true)` for `recursive` (line 110) and the `useEffect` that
adopts paths on opening the dialog (`open`/`win` payload) and starts the first
comparison (line ~150–162) hardcoded "Recursive" to `true` and
started the first `compareDirs` run with `rec=true` as well. Fix: both
places changed to `false`. A previously manually set state could
never "carry through" anyway (the `useEffect` unconditionally overwrites `recursive` on
every open) — the actual bug was merely the
wrong default value.

No Rust/backend changes needed (`compareDirs(l, r, rec, …)` gets the
recursive value from the frontend as an explicit parameter anyway).

Verified: `npm run typecheck`, `npm run build` succeed; in
browser demo mode a full run (default state disabled →
enabling takes effect → close/reopen resets to disabled),
no console errors.
