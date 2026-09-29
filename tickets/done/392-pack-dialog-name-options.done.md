# TICKET-392: A pack dialog that composes the archive name

Priority: **P2**. Status: **done**.
Reported from use on 2026-09-29, as the follow-up to TICKET-390: the proposal
is now a good name, but it is take-it-or-retype-it.

## Problem

Packing reuses the shared `PromptDialog` (`runPack`,
`src/features/commander/fileOps.ts:430-461`): one text field carrying
`<base>_YYYY-MM-DD_HH-MM-SS.zip`, nothing else. The name is composed once, in
`proposedArchiveName`, and from then on the only way to change any part of it
is to edit the string by hand.

- The base is always derived from the selection. A user who wants the plain
  `archiv` back has to delete the folder name out of the middle of the string.
- Date and time always come along. Packing `Fotos/` as simply `Fotos.zip` — the
  common case when the archive is going to be sent to someone — means deleting
  a 19-character timestamp, and getting the remaining underscore right.
- The dialog is a single 520px-wide line for a decision that has three parts,
  so the parts are invisible: nothing in the dialog says that the timestamp is
  optional, or where the base came from.

## Required change

A pack dialog of its own, sized for its contents, that composes the name from
options instead of only offering a finished string:

- **Base**: a choice between the name derived from the selection (the folder
  or file that TICKET-390 already picks) and the generic `archiv`. The choice
  shows both actual bases, so what each one produces is readable before it is
  picked.
- **Add date** and **Add time**: two checkboxes, both on by default, so the
  dialog opens with exactly the name TICKET-390 proposes. Turning both off
  gives `<base>.zip`; the order of the parts stays `<base>_<date>_<time>.zip`.
- The name field shows the composed name and stays editable. **Editing it by
  hand switches the dialog to manual mode**: the option controls are disabled,
  because they no longer describe what is in the field, and a disabled control
  that silently stops mattering is worse than one the user can see is off.
- A checkbox **takes the field back over**: checking it leaves manual mode,
  re-enables the options and rebuilds the name from them, discarding the
  hand-typed value. It is checked while the options drive the field and
  unchecks itself the moment the user types — one control, two directions.
- The composed name is still the value that is packed, with the existing
  `.zip` suffixing and blank check unchanged, and the field still opens
  focused with everything but `.zip` preselected.
- The timestamp is captured once when the dialog opens, so toggling date and
  time off and on again yields the same name rather than a drifting one.

The name composition itself stays a pure function in `archiveName.ts`, tested
without a dialog, the way TICKET-390 left it.

## Verification

- Frontend test: `composeArchiveName` for each combination of base, date and
  time, including both parts off.
- Frontend test: the dialog opens with the proposal and both options on;
  turning the date off rebuilds the name without it; choosing the generic base
  rebuilds it with `archiv`; typing into the field disables the option
  controls; the takeover checkbox re-enables them and restores the composed
  name; submitting packs to the name in the field.
- Frontend test: `runPack` opens the pack dialog with the packed rows in list
  order and packs into the other panel's path.
- `npm run check`.

## Implementation

`PackDialog.tsx` is a dialog of its own rather than another option on the
shared `PromptDialog`: the prompt is used by rename, mkdir, the archive
password and the selection patterns, and none of them wants a base, a date and
a time. `runPack` now raises a `PackRequest` (`opsStore.ts`) carrying the
packed rows in list order; the base is derived inside the dialog, where it can
change while the dialog is open.

- `archiveName.ts` gained `composeArchiveName(entries, options, now)` plus
  `selectionBase`/`genericBase` for the two choices the select offers.
  `proposedArchiveName` is now that composition under `DEFAULT_NAME_OPTIONS`,
  so the dialog opens on exactly the TICKET-390 name and the old tests still
  describe the default.
- The dialog keeps one boolean, `auto`. It drives three things at once: the
  option controls' `disabled`, the takeover checkbox's own checked state, and
  whether an option change rewrites the field. Typing sets it to false; the
  checkbox sets it back to true and recomposes. A second "manual" flag beside
  the checkbox would have been two sources for one fact.
- `openedAt` is a ref taken when the dialog opens, not `new Date()` per
  composition: with a fresh clock, switching the time off and on again would
  hand back a different name than the one it took away.
- **Lazy, and with its own text.** `OperationDialogs` stays eager on purpose
  (a confirm must never wait on a fetch), but pack always follows a deliberate
  Alt+F5, so the dialog sits behind `LAZY.pack` in `lazyFeatures.ts` and mounts
  from `LazyDialogs.tsx`. The entry chunk was 0.1 kB under its 325 kB budget
  before this ticket, and an eager pack dialog put it 3.8 kB over. Its seven
  strings moved with it into `i18n/namespaces/pack.ts`, registered on import
  the way `rename` and `preview` already do — entry chunk 324.5 kB.
- `SimpleDialog` moved out of `OperationDialogs.tsx` into its own module so the
  lazy dialog can share the chrome without importing the eager file back.

### Verification

- `archiveName.test.ts`: all six combinations of base, date and time including
  both parts off (`Fotos.zip`, `archiv.zip`), both bases the select offers, and
  that the default proposal is still the composed one.
- `PackDialog.test.tsx`: opens on the proposal with all but `.zip` preselected
  (and clean of serious/critical axe violations); switching date, time and base
  rebuilds the name; typing disables the select and both checkboxes and shows
  the takeover hint; the takeover re-enables them and restores the composed
  name; Enter submits the field's value; a blank field disables "Create" and
  Escape closes without submitting.
- `fileOpsErrors.test.ts`: `runPack` hands the dialog the rows in list order
  (not marking order) and its submission packs into the other panel's path.
- Checked by hand in the browser demo build: Alt+F5 on `Pictures` opens the
  dialog through its lazy boundary, unchecking "Datum anhängen" rewrites the
  name to `Pictures_13-04-57.zip`, typing greys the options out, and the
  takeover checkbox brings the composed name back.
- `npm run check` passed on macOS (arm64), bundle report within budget.
