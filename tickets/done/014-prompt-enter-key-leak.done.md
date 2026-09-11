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
- [x] `F7` → name → `Enter` creates the folder, the panel stays in the same
      directory, and the new folder is selected.
- [x] Reproduces neither with the cursor on `..` nor on any other row (no entry
      is opened by the confirming keystroke).
- [x] `F2` rename confirmed with `Enter` does not navigate or open anything.
- [x] Pattern select (`+`/`-`) and the archive password prompt confirmed with
      `Enter` do not leak either.
- [x] `Escape` in a prompt dialog closes only the dialog and does not clear the
      pane's quick filter.
- [x] Confirming via the **Create** button still works unchanged.
- [x] `Enter` in the file pane with no dialog open still opens the cursor entry.

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

## Implementation

**Root cause, both ends.** `PromptDialog` (`OperationDialogs.tsx`) now consumes
`Enter` and `Escape` via the new `consumeDialogKey` (`src/lib/keys.ts`):
`preventDefault()` + `stopPropagation()` — React forwards the latter to the
native event, so the keystroke never reaches the `window` listener. That alone
would only fix the symptom, so the global handler was made independent of state
that the unmount destroys:

* `lib/keys.ts` keeps a `WeakSet` of keydown events a dialog has claimed.
  `markDialogKey` is wired as `onKeyDownCapture` on **every** `Dialog.Content`
  (Operation dialogs, `AppDialog`, FileBrowser, Preview, Properties, Search), so
  the mark is set before any handler inside the dialog can close it. The event
  object outlives the unmount, unlike `document.activeElement` or the DOM
  position of the target.
* `useKeyboard.ts` bails out on a marked event first, then reads `e.target`
  instead of `document.activeElement` for its input/`[role="dialog"]` guards,
  and finally ignores every keystroke while any `[role="dialog"]` is in the
  document — that last guard also covers the frame between opening a dialog and
  its input taking focus (`PromptDialog` focuses in a `requestAnimationFrame`).

Because there is a single prompt component, this covers all `requestPrompt`
consumers at once: New Folder, Rename, pattern selection (`+`/`-`) and the
archive password prompt.

**Other `Enter`-submitting inputs** were reviewed: Search (name + content),
FileBrowser (path), Properties (tag editor) and Settings (editor extensions)
do not unmount on the keystroke, but they now consume it the same way instead of
letting it bubble to `window`. `ConfirmDialog` does not submit on `Enter` at all
(the keystroke is swallowed by the dialog content) — unchanged, and its content
carries the capture mark.

**Reveal after mkdir/rename.** `panesStore.refresh` accepts `names` in addition
to `reveal`; `runMkdir`/`runRename` pass the new name. The reveal no longer
depends on diffing against the previous listing, where a watcher refresh landing
in between would make the entry look "not new" and leave the cursor where it was.

### Verified

`npm run typecheck`, `npm run build` and `cargo check` all pass (no Rust changes).

UI verified in the Browser pane (vite demo mode), in `/Users/demo/Documents`:

* `F7` → name → `Enter`: folder created, panel stays in the directory (breadcrumb
  unchanged), dialog closed — with the cursor on `..` and, separately, on a file
  row (no `open_path` in the console, i.e. no entry was opened).
* A `keydown` probe on `window` shows the confirming `Enter` never arrives there;
  only the `F7` that opened the dialog does.
* `F2` → `Enter`: `rename_entry` runs, no navigation, nothing opened.
* Pattern selection (`+`) → `*.md` → `Enter`: `readme.md` marked ("1 von 11"),
  no navigation.
* `Escape` in the prompt with an active quick filter (`d`): dialog closes, the
  filter stays — pressing `Escape` again in the pane clears it as before.
* The **Create** button path is unchanged (mouse confirm closes the dialog, no
  console errors).
* `Enter` in the pane with no dialog open still opens the cursor entry.
* Search: `Enter` in the name field still starts the search ("3 Treffer") and is
  consumed.

Not reachable in demo mode and therefore unproven: the archive password prompt
(needs an encrypted archive) and mkdir on a remote VFS location — both go through
the same `PromptDialog`, which is the only place the keystroke is handled. The
reveal itself is likewise not observable in demo mode, because the demo listing
is static and never contains the created folder.
