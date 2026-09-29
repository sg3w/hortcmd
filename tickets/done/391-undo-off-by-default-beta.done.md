# TICKET-391: File undo ships off by default and is marked BETA

Priority: **P1**. Status: **done**.
Requested on 2026-09-29: every undo function is to be **off by default** and
marked as **BETA** in the settings.

## Problem

File undo is on unconditionally. There is no switch anywhere — not in the
settings, not in the backend — and the feature is reachable the moment the app
starts:

- `undo::prepare` / `prepare_group` are called from `mkdir`, `rename`,
  `rename_batch_on`, the permissions, owner and Finder-tag commands and from
  the transfer plan (`commands/fs/file.rs:29,77,263`, `props.rs:199,232`,
  `tags.rs:56`, `transfer/plan.rs:174`). Every one of them writes a journal
  entry, and copy undo deliberately **retains the copied bytes** in a
  `.hortcmd-undo-*` sibling (`docs/file-undo.md`). So a user who has never
  heard of the feature accumulates a journal and retained data on disk.
- `Dateien → Dateiaktion rückgängig machen`, its ⌘⌥Z shortcut
  (`useKeyboard.ts:139`) and the `file.undo` command (`commands.ts:473`) are
  always offered.
- Every copy/move confirmation and every rename/mkdir prompt runs
  `undo_classify` and prints a reversibility sentence
  (`OperationDialogs.tsx:202,300`), so the feature is visible in dialogs that
  have nothing else to do with it.

The adapter itself is macOS-only, conditional by its own contract, and its
Windows/Linux halves are still open ([119](119-file-undo-windows-linux-adapters.md)).
That is a beta feature presented as a finished one, and it is on for everybody.

## Required change

- One setting, `undoEnabled`, **off by default** and off for every existing
  installation (an absent stored value is the default, not a repair). The
  Settings ▸ File operations page carries it with a **BETA** badge and a hint
  that names both the limits (local macOS only, conditional) and the cost
  (journal plus retained copies on disk).
- The backend holds the same state and defaults to off there too, so the
  journal is not written by a frontend that never got the chance to say
  anything — a browser build, a crashed renderer, a plugin or MCP client
  driving a file command. A new `undo_set_enabled` command is what the
  frontend pushes the setting through, at startup and on every change.
- While undo is off:
  - `prepare`, `prepare_group` and `seal` record nothing, so no journal entry
    and no retained copy is created;
  - `undo_classify` answers `Irreversible`, which is what it is — nothing is
    being recorded;
  - `undo_preview` answers with an empty history, and the mutating commands
    `undo_execute` and `undo_forget` refuse, so a stale window or a
    non-UI caller cannot execute an undo behind the switch;
  - the `file.undo` command is hidden from the menu and the keyboard overview,
    ⌘⌥Z does nothing, and the dialogs print no reversibility sentence at all
    rather than "cannot be undone".
- Turning the switch off does **not** delete an existing journal or a retained
  copy. Both stay where they are and become reachable again when the switch is
  turned back on; deleting user data on a settings change is not this ticket's
  to do.

## Verification

- Rust: the gate defaults to off; `prepare`/`prepare_group` return no pending
  entry and `seal` succeeds while off; `undo_preview` is empty and
  `undo_execute`/`undo_forget` refuse while off; and the flag round-trips
  through `undo_set_enabled`.
- Frontend: `undoEnabled` defaults to `false` and survives a stored `true`;
  the File-operations page shows the BETA toggle and writes the store;
  `openUndo()` does nothing while off; `file.undo` is hidden while off and
  offered while on; `useUndoClassification` performs no IPC and reports
  `undefined` while off.
- `npm run check`.

## Implementation

The gate exists twice, on purpose. The frontend one decides what is *offered*;
the backend one decides what is *recorded*, and they are not the same question.

**Backend** (`commands/fs/undo/mod.rs`). A process-wide `AtomicBool`, default
`false`, and a `undo_set_enabled` command the frontend pushes it through. A
per-call flag was the alternative and does not fit: the journal is written from
seven commands (`mkdir`, `rename`, `rename_batch_on`, permissions, owner, tags,
the transfer plan), most of which carry no settings at all, and adding one to
`RecoverySettings` would also have changed what a persisted plan file contains.
The default matters more than the mechanism — a backend that starts off records
nothing for a browser build, for a renderer that dies before it can speak, and
for a plugin or MCP client driving a file command.

Where the gate sits per entry point follows what each one does:

- `prepare`, `prepare_group` → `Ok(None)`, and `seal` → `Ok(())`. No journal
  entry, no retained copy, and the callers already handle "no pending entry"
  because unsupported providers produce exactly that.
- `undo_classify` → `Irreversible`. That is the truth while nothing is being
  recorded, not a placeholder.
- `undo_preview` → an empty history, because reading is harmless and an empty
  list is what the non-macOS build already answers.
- `undo_execute` and `undo_forget` → refused. They mutate, so a window left
  open across a settings change, or any non-UI caller, is turned away rather
  than trusted.

**Frontend.** `undoEnabled` is a persisted setting, `false` in the store and
`false` in the validator, so a fresh installation and an existing one agree —
the mistake TICKET-029's default made and TICKET-039 had to migrate away. No
schema version bump: an absent value is an older installation meeting a new
field, which is what a default is for, and counting it as a repair would raise
TICKET-389's recovery notice for everyone.

`undoStore.ts` owns the gate rather than each caller: `setOpen` refuses to open
while off, so `openUndo`, the `file.undo` command and anything added later pass
through one check. On top of that, `file.undo` reports `hidden` (a feature
nobody switched on has no reason to occupy a menu line, and the keyboard
overview filters hidden commands for free), and ⌘⌥Z tests the gate *before*
`preventDefault`, so the key is not swallowed by a feature that is not running.

The dialogs say nothing at all while off. `useUndoClassification` returns
`undefined` — the value `OperationDialogs` already treats as "no sentence" —
and performs no IPC; `RenameDialog` and `PropertiesDialog` drop their static
`undo.conditional` line the same way. Printing "cannot be undone" instead would
describe a limit of the feature rather than its absence.

`useUndoGate` (mounted once in `App.tsx`, beside `useFileColorVars`) pushes the
setting to the backend at startup and on every change, and closes an open
history when the switch goes off — the commands behind it refuse from that
moment on. It deletes nothing: an existing journal and its retained copies stay
on disk and are reachable again when the switch comes back.

The settings entry is on the File-operations page with a new `Beta` badge. The
badge sits beside the label, not inside it, so the label remains the switch's
accessible name and a screen reader does not read "BETA" as part of every state
announcement. The standing amber warning belongs to the **on** state here —
unlike failure-safe transfers, where off is the costly state — because being on
is what writes journal entries and leaves retained copies behind.

### Verification

- `undo::gate_tests` (all platforms, one test because the flag is process-wide):
  starts off; `prepare`/`prepare_group` yield no pending entry and `seal`
  succeeds; `undo_preview` is empty, `undo_execute`/`undo_forget` refuse and
  `undo_classify` answers `Irreversible`; the flag round-trips through
  `undo_set_enabled`.
- `persisted.test.ts`, "ships file undo off, for existing installations too":
  the store default, a fresh load, a v0 load and a non-boolean stored value all
  give `false`; an explicit `true` survives.
- `sections.test.tsx`, "offers file undo off, marked BETA, and warns once it is
  on": the BETA badge, the switch at `aria-checked="false"`, no warning while
  off, and both the store write and the warning after the click.
- `UndoDialog.test.tsx`, "offers no entry point while file undo is switched
  off": ⌘⌥Z does nothing, `openUndo()` does nothing, `file.undo` is hidden —
  and all three reverse when the setting is turned on.
- `undoGate.test.tsx`: `undo_set_enabled` is invoked with `true` and then with
  `false`, an open history closes when it goes off, and
  `useUndoClassification` reports `undefined` without calling `undo_classify`
  while off, then `conditional` once on.
- `RenameDialog.test.tsx`: the `undo.conditional` sentence appears only while
  the setting is on.
- `npm run check` passed on macOS (arm64).
