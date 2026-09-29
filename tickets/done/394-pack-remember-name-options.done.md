# TICKET-394: The pack dialog forgets the options on every archive

Priority: **P2**. Status: **done**.
Reported from use on 2026-09-29, from packing a folder of projects one by one:
the options TICKET-392 added have to be set again for every single archive.

## Problem

`PackDialog` resets to `DEFAULT_NAME_OPTIONS` whenever it opens
(`src/components/ui/PackDialog.tsx:48-53`). Base, "add date" and "add time" are
therefore not settings but a form that is cleared each time, and the one case
that makes the options worth having — packing many folders in a row, each as
plain `<name>.zip` — is the case that pays for them twice per archive: two
clicks before every Alt+F5, forever, with nothing in the dialog remembering that
the last fifty archives were named the same way.

## Required change

- The three option values — base, date, time — are a persisted setting. The
  dialog opens with what was chosen last, on the next archive and after a
  restart, so it starts as the user left it.
- Choosing an option stores it right away, rather than only on a successful
  pack: what the dialog shows on the next opening is what the user last picked,
  whether or not that archive was created.
- The name in the field stays composed from those options, so a remembered
  "no timestamp" opens on `<base>.zip` with a fresh timestamp available again
  by checking the box.
- The manual mode is not part of it: the dialog always opens in composing mode
  (`auto`), because a hand-typed name from a previous archive is exactly what
  must not be offered again.
- The stored value is validated on the way in like every other setting — an
  unknown base or a non-boolean falls back to the TICKET-392 default instead of
  reaching the composition.

## Verification

- Frontend test: the dialog opens with stored options (generic base, no date)
  rather than the defaults, and composes the name from them.
- Frontend test: turning "add time" off stores it; the next opening starts
  without it, and after closing the dialog without packing as well.
- Frontend test: a stored value with an unknown base is repaired to the default.
- `npm run check`.

## Implementation

`packNameOptions` is an ordinary persisted setting (`settingsSchema.ts`,
`settingsStore.ts`, validated in `persisted/settings.ts`), and `PackDialog`
reads it when it opens and writes it on every option change.

- **Read and written through `getState`, not subscribed.** A subscription would
  re-run the opening effect on the store write the dialog itself just made, and
  that effect rebuilds the field — including over a name the user had typed.
  The dialog already owns the options as state while it is open; the store is
  where they go, not a second source for them.
- **Written on the change, not on the pack.** Otherwise a cancelled dialog
  forgets a decision the user did make, and the series this ticket is about —
  fifty folders, each `<name>.zip` — pays for the one archive that was thought
  better of.
- **`auto` stays per-opening.** The composing mode is how the dialog works, not
  a preference; a hand-typed name from a previous archive is the one thing that
  must not come back.
- **The default is spelled out three times on purpose.** `settingsStore` and the
  validator are eager, `DEFAULT_NAME_OPTIONS` sits in the lazy pack feature, and
  one value import pulled that feature into the entry chunk — 325.5 kB against a
  325 kB budget. A test holds the three together instead
  (`"ships the pack options the dialog composes with"`), which is what the
  failure-safe default already does for `safeTransfers`.

### Verification

- `PackDialog.test.tsx`: the dialog opens on stored options (generic base, no
  date) and composes from them; unchecking "add time" stores it, and after
  Escape — nothing packed — the next opening starts without the time.
- `persisted.test.ts`: store default, validator default and
  `DEFAULT_NAME_OPTIONS` agree; an unknown base and a non-boolean are repaired
  part by part (two repairs, `date: false` survives); a stored choice is kept
  with no repairs; an absent field is still not a repair.
- Checked by hand in the browser demo build: Alt+F5 on `Pictures`, both boxes
  off → `Pictures.zip`; Escape and Alt+F5 again opens with both still off; after
  a full reload of the page it is still `Pictures.zip`.
- `npm run check` passed on macOS (arm64), entry chunk 324.8 kB, within budget.
