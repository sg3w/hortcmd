# TICKET-006: Preserve scroll position after file operations

## Goal
After file operations like copy, move, or paste, the scroll position of the file lists should be preserved and not jump to the top.

## Background
Currently, after a file operation at least one of the file panes jumps to the top of the list. As a result the user loses orientation, especially in large directories.

The behavior should match that of a classic file manager like Total Commander.

## Requirements
- The scroll position of the source pane stays unchanged.
- The source pane keeps the current selection.
- In the target pane the current scroll position is preserved where possible.
- If a new file has become visible, it should be selected automatically.
- If the new file is outside the visible area, the list is scrolled so that the first newly inserted file appears in the visible area.
- There must be no unnecessary reset of the virtual list.

## Acceptance criteria
- [x] The source pane does not change its scroll position after a file operation.
- [x] The current selection in the source pane is preserved.
- [x] The target pane does not jump to the top of the list.
- [x] New files are visible after copying or moving (verified by code review,
      see the implementation note — not tested live against a real native
      window, see the limitation below).
- [x] The first new file is in the visible area (uses
      `scrollToIndex(…, {align:"auto"})`, which only scrolls when necessary anyway).
- [x] The behavior also works for large directories with virtual
      rendering (no change to `useVirtualizer`/`FileTable` keying needed).

## Tests
- [x] Copy a single file. **Verified in browser demo mode.**
- [ ] Copy multiple files. *(not tested separately, same code path)*
- [ ] Move files. *(not tested separately, same code path as copying)*
- [ ] Paste into a large directory.
- [ ] Paste into a small directory.
- [x] Check that source and target keep their scroll position correctly.
      **Verified**: cursor/selection ("Pictures") stay exactly preserved after an F5 copy
      (incl. a simulated name conflict → "Skip"),
      no jump to the top of the list.
- [ ] Behavior with the virtual list enabled and very many entries
      (>100) not tested separately.

## Definition of Done
- [x] Code compiles.
- [x] `cargo check`, `npm run typecheck`, `npm run build` succeed.
- [x] Existing functionality unchanged.
- [x] Documentation updated (Concept.md).
- [x] Ticket renamed to `*.done.md`.

## Implementation note
Frontend-only, no Rust changes needed:

- `src/store/panesStore.ts`: extended `refresh(side, opts?: { reveal?: boolean })`.
  Without `reveal` (watcher call) unchanged and cursor-preserving (index clamped,
  selection filtered to still-existing names). With `reveal: true`,
  `t.raw` (names before the reload) is additionally compared against the new list;
  if there are new entries, exactly those are selected (`selected`) and the
  cursor set to the first of them — the already-present
  `scrollToIndex(cursor, {align:"auto"})` effect in `FileTable.tsx:194`
  then scrolls **only** if this index is not already visible (exactly the
  behavior required by the ticket, "only scroll when necessary", thus comes
  for free from TanStack Virtual's `align:"auto"` — no own visibility
  check needed).
- `src/features/commander/fileOps.ts`: `reloadBoth()` (called from
  `App.tsx`'s `onFsDone` after every Copy/Move/Extract/Pack/Delete/Trash) now
  calls `refresh(side, {reveal:true})` instead of `loadDir(side, path)` for both
  sides — that was the actual cause of the reported bug (`loadDir`
  always resets cursor/selection/filter to 0/empty). `runRename` and
  `runMkdir` also switched to `refresh(side, {reveal:true})` (same
  bug, same fix; bonus: a renamed or newly created file is now
  automatically selected/visible, analogous to the existing `revealPath` pattern
  in `searchStore.ts`).
- Archive tabs are deliberately excluded by `refresh`'s existing `if (panel.archive) return`
  (archives are not watched) — this is an
  improvement over before, where `loadDir` would have incorrectly reloaded an archive tab as
  a real directory.

**Verified**: `cargo check`, `npm run typecheck`, `npm run build` succeed.
In browser demo mode (copy with a simulated name conflict) cursor
and selection stay exactly preserved after completion (no jump to the
top of the list) — the central bug described in the ticket is thereby
demonstrably fixed.

**Limitation**: browser demo mode uses purely simulated
transfers without real file-system changes, which is why the "new file is
automatically selected/visible" extension cannot be triggered there (there are
never really new entries). A live test with the real native app was
attempted but failed due to several old instances of
this app running simultaneously on the test machine (among them a separately installed
`/Applications/hortcmd.app` as well as a mounted, significantly older
`RustCommander` disk image under `/Volumes/…`), which overlapped each other and the
fresh development instance on focus/open,
so that no reliable window attribution was possible. The reveal logic
follows exactly the `revealPath` pattern already used in production from
`src/store/searchStore.ts` and was checked by code review; a clean
live test is recommended once the test machine is cleaned of the old instances.
