# TICKET-396: Transfer recovery ships off by default and is marked BETA

Priority: **P1**. Status: **done**.
Requested on 2026-09-30: the "Unterbrochene Übertragungen" feature is to be an
**option, off by default**, and marked as **BETA**.

## Problem

Recovery of interrupted transfers (TICKET-044) is on unconditionally. There is
no switch anywhere — not in the settings, not in the backend — and the feature
reaches the user before they have done anything:

- every copy and move registers its intent before the first byte moves
  (`fileOps.ts:186` → `recovery_queue_transfer`), and extraction and packing
  journal themselves the same way (`archive.rs:308,385` and the transfer engine
  at `engine.rs:62`, all through `recovery::optional_begin`). Each run writes a
  journal record, and a failure-safe transfer's preflight plan is stored beside
  it.
- `App.tsx` asks the journal at every start and **opens the dialog by itself**
  when anything is pending, so a user who has never heard of the feature is
  asked to resolve "Aufträge" they did not know were being recorded.
- `Commands ▸ Interrupted transfers` (`commands.ts:476`) is always offered.

The adapter is macOS-only (`recovery::supported`) and its Windows/Linux half is
still open ([116](../116-windows-linux-transfer-recovery.md)). That is a beta
feature presented as a finished one, and it is on for everybody.

## Required change

- One setting, `recoveryEnabled`, **off by default** and off for every existing
  installation (an absent stored value is the default, not a repair). It sits on
  the Settings ▸ File operations page beside the file-undo switch, with a
  **BETA** badge and a hint that names the limit (macOS only for now) and the
  cost.
- The backend holds the same state and defaults to off there too, so nothing is
  journaled by a frontend that never got the chance to say anything — a browser
  build, a crashed renderer, a plugin or an MCP client driving a transfer. A new
  `recovery_set_enabled` command is what the frontend pushes the setting
  through, at startup and on every change.
- While transfer recovery is off:
  - no journal record is written for any copy, move, extraction or packing run;
  - the **preflight check in front of a transfer still runs** — it is a check on
    the operation, not on its recovery, and dropping it would trade one
    feature's switch against another's safety;
  - `recovery_list` answers empty and `recovery_prepare` refuses, so a stale
    window or a non-UI caller cannot restart a journaled job behind the switch;
  - the `cmd.recovery` command is hidden and the startup dialog never opens.
- Turning the switch off deletes **no** journal record and no staged data. Both
  stay where they are and become reachable again when the switch comes back;
  deleting user data on a settings change is not this ticket's to do.

## Verification

- Rust: the gate defaults to off, `optional_begin` records nothing while off
  (and does not even build the intent), and the flag round-trips through
  `recovery_set_enabled`.
- Frontend: `recoveryEnabled` defaults to `false` and survives a stored `true`;
  the File-operations page shows the BETA switch, warns only once it is on, and
  writes the store; `openRecovery()` does nothing and `cmd.recovery` is hidden
  while off; the gate pushes the switch to the backend, reads the journal only
  once it is on, and closes an open list when it goes off.
- `npm run check`.

## Implementation

The gate exists twice, as in TICKET-391: the frontend one decides what is
*offered*, the backend one what is *recorded*.

**Backend** (`transfer/recovery/mod.rs`). A process-wide `AtomicBool`, default
`false`, and a `recovery_set_enabled` command. A per-call flag does not fit:
recording starts in three places that carry no settings of their own — the
transfer engine, `archive.rs`'s extraction and packing — and `RecoverySettings`
is written into the journal record itself, so adding a field there would have
changed what a persisted plan contains.

Where the gate sits per entry point follows what each one does:

- `optional_begin` → `Ok(None)`, beside the existing `supported()` check. Every
  caller already handles "no run" because the non-macOS build produces exactly
  that, so one line covers copy, move, extraction and packing. The intent
  closure is not even invoked.
- `recovery_queue_transfer` → returns **after** the preflight. This is the one
  deliberate asymmetry with TICKET-391: the command does two jobs, and only one
  of them belongs to the feature. `preflight::build` + `ensure_executable` +
  `matches_review` verify that the transfer the user approved is still the
  transfer that will run; the journal is what outlives it. Only the journal is
  withheld.
- `recovery_list` → an empty list, which is what the non-macOS build already
  answers and what keeps the startup dialog shut.
- `recovery_prepare` → refused. It is the one mutating step behind the switch
  (it enqueues a retry and hands back a launch), so a window left open across a
  settings change is turned away rather than trusted.
- `recovery_cleanup`, `recovery_discard`, `recovery_acknowledge` → unchanged.
  They only ever *remove* a record, and turning the feature off must not be able
  to strand one. `acknowledge` is also called from the normal transfer-done path,
  which the frontend now skips while off so the journal storage is not created
  for a feature that is not recording.

**Frontend.** `recoveryEnabled` is a persisted setting, `false` in the store and
`false` in the validator, so a fresh installation and an existing one agree. No
schema version bump: an absent value is an older installation meeting a new
field, and counting it as a repair would raise TICKET-389's recovery notice for
everyone.

`recoveryStore.ts` owns the gate rather than each caller: `setOpen` refuses to
open while off and `refresh` clears itself and returns, so `openRecovery`, the
`cmd.recovery` command and anything added later pass through one check. The
command reports `hidden` for the reason `file.undo` does — a feature nobody
switched on has no reason to occupy a menu line.

`useRecoveryGate` (mounted in `App.tsx` beside `useUndoGate`) pushes the setting
and **owns the startup look at the journal**, which `App.tsx` used to do in an
effect of its own. That move is the point: asking the backend before it has been
told the switch is on would always answer "nothing pending" and lose the one
moment the question matters. Only the first push counts as a startup, so turning
the switch on from the settings reloads the list without throwing a dialog over
the page the user is standing on.

The settings entry reuses `recovery.title` as its label instead of adding a
second name for the same feature, and the standing amber warning belongs to the
**on** state, as with file undo: being on is what writes records and what makes
an interruption something the user has to answer.

### Verification

- `recovery::gate_tests::recording_is_off_until_the_switch_is_pushed` (all
  platforms, one test because the flag is process-wide): starts off,
  `optional_begin` yields no run and never calls its intent closure, and the
  flag round-trips.
- `persisted.test.ts`, "ships transfer recovery off, for existing installations
  too": the store default, a fresh load, a v0 load and a non-boolean stored
  value all give `false`; an explicit `true` survives.
- `sections.test.tsx`, "offers transfer recovery off, marked BETA, and warns
  once it is on". The file-undo case in the same file now asserts its BETA badge
  on its own row rather than globally — the page carries two beta switches.
- `recoveryGate.test.tsx`: the switch is pushed at startup, the journal is read
  only once it is on, an open list closes when it goes off, and while off
  `openRecovery()` does nothing, `cmd.recovery` is hidden and no IPC happens.
- `recoveryStore.test.ts` turns the setting on — those two tests are about what
  the feature does once it is enabled.
- `npm run check`: green except `check:bundle`, which is **already red on
  `main`** — the entry chunk is 325.1 kB against a 325 kB budget. This ticket
  takes it to 326.3 kB: two hint strings in the eager `settings` namespace plus
  the gate itself. Raising `EAGER_BUDGET_KB` is a deliberate decision the report
  asks to be recorded in Concept.md §4 and is left to the ticket that fixes the
  pre-existing overrun.
