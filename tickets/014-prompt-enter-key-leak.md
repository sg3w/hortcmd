# TICKET-014: Enter in the "New folder" dialog leaks to the file pane

## Goal
Confirming a prompt dialog with `Enter` must only execute that dialog's action.
Creating a folder with `Enter` currently also triggers "open cursor entry" in the
file pane behind it, which — with the cursor on the `..` row — navigates one
directory up. The folder is created, but the panel is no longer showing the
directory the user created it in.

## Reproduction
1. Enter any subdirectory (the cursor sits on the `..` row).
2. Press `F7`, type a folder name, confirm with `Enter`.
3. The folder is created, but the panel jumps to the parent directory instead of
   revealing the new folder.

Confirming the dialog with a mouse click on **Create** behaves correctly — the
bug only occurs on the keyboard path.

## Analysis
The prompt input in `PromptDialog`
(`src/components/ui/OperationDialogs.tsx:213`) handles `Enter` with

```tsx
onKeyDown={(e) => { if (e.key === "Enter") submit(); ... }}
```

without `preventDefault()`/`stopPropagation()`. `submit()` calls `closeDialogs()`
synchronously, which unmounts the dialog while the very same native keydown
event is still propagating.

The global TC key handler (`src/features/commander/useKeyboard.ts:238`) is
registered on `window`, i.e. above the React root, so it receives that event
*after* the dialog is gone. Its two guards both look at `document.activeElement`
— an input/textarea, or an ancestor with `role="dialog"`
(`useKeyboard.ts:45-54`). With the dialog unmounted, `activeElement` has fallen
back to `<body>`, so neither guard matches and the handler proceeds to
`case "Enter": openCursor()` (`useKeyboard.ts:170`). `openEntry` on the `..` row
runs `loadDir(side, parentPath(p.path))` (`navigate.ts:35`) — the jump upwards.

The same leak applies to every consumer of `requestPrompt`, not just `mkdir`:
rename (`fileOps.ts:421`), pattern select (`selection.ts:23`), the archive
password prompt (`archivePw.ts:37`). `Escape` in the same handler leaks the same
way (it currently only clears the quick filter, so it is invisible today, but it
is the same defect).

Other dialogs submit on `Enter` in the same shape and should be checked against
the same failure mode: `SearchDialog.tsx:178,195`, `FileBrowserDialog.tsx:172`,
`PropertiesDialog.tsx:775`, `SettingsDialog.tsx:1126`.

## Requirements
- Confirming or cancelling a prompt dialog by keyboard must not reach the global
  key handler. Stop the event where it is handled — `preventDefault()` **and**
  `stopPropagation()` for both `Enter` and `Escape` in the prompt input.
- Fix the root cause, not only the symptom: the global handler must not act on
  an event that a dialog already consumed, even when the dialog unmounts
  mid-dispatch. Prefer a guard that does not depend on `document.activeElement`
  surviving the dispatch — e.g. checking the open-dialog state in the store, or
  reading `e.target` instead of `document.activeElement`.
- The fix must cover all `requestPrompt` consumers (mkdir, rename, pattern
  select, archive password) — there is one dialog component, so one fix.
- Review the other `Enter`-submitting dialogs listed above and apply the same
  treatment where they can close or unmount on the same keystroke.
- After a successful mkdir, the panel stays in the current directory and the new
  folder is revealed and put under the cursor — `refresh(side, { reveal: true })`
  already intends this (`fileOps.ts:465`); make sure the reveal is what the user
  actually sees.
- No behavioural change for the mouse path or for `Enter` in the file pane when
  no dialog is open.

## Acceptance criteria
- [ ] `F7` → name → `Enter` creates the folder, the panel stays in the same
      directory, and the new folder is selected.
- [ ] Reproduces neither with the cursor on `..` nor on any other row (no entry
      is opened by the confirming keystroke).
- [ ] `F2` rename confirmed with `Enter` does not navigate or open anything.
- [ ] Pattern select (`+`/`-`) and the archive password prompt confirmed with
      `Enter` do not leak either.
- [ ] `Escape` in a prompt dialog closes only the dialog and does not clear the
      pane's quick filter.
- [ ] Confirming via the **Create** button still works unchanged.
- [ ] `Enter` in the file pane with no dialog open still opens the cursor entry.

## Tests
- Manual walkthrough of every acceptance criterion above, in both panels, once
  with the cursor on `..` and once on a regular entry.
- Same walkthrough with an active quick filter, to cover the `Escape` path.
- Regression: `Enter` inside the command line / other text inputs still behaves
  as before.
- Regression: mkdir on a remote VFS location (SFTP/SMB) behaves the same.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked)
- Rename the ticket to `014-prompt-enter-key-leak.done.md`
