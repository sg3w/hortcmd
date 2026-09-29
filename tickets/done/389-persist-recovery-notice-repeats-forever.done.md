# TICKET-389: Stop the persisted-state recovery notice from firing on every start

Priority: **P1**. Status: **open**.
Reported from use on 2026-09-29: "Stored data was unusable and has been reset"
stands on the screen at every launch, and there is no way to turn it off.

## Problem

Three defects compound into a red error toast the user cannot get rid of.

### 1. A repair is never written back, so it is made again forever

`rehydrate` (`src/store/persisted/gate.ts:150-170`) validates the stored value,
repairs it, records the repair and returns the repaired value — into the
store's `merge`. Zustand's `persist` does not write on rehydration, so nothing
puts the repaired value back into `localStorage`. The stored text still holds
whatever needed repairing, and the next start repairs it again and raises the
same notice again. The comment above `quarantine` argues for not overwriting
the original, and it is right about the *quarantine copy* — but the live key is
then left holding data the application has already decided is wrong.

### 2. Something ordinary keeps needing that repair

`AppDialog`'s `clamp` bounds a dialog to 90 % of the window:

```ts
const maxW = Math.max(min.w, window.innerWidth * MAX_FRAC);
```

`window.innerWidth * 0.9` is fractional for every odd window width, and the
clamped value is what `setSize` persists on mouse-up
(`src/components/ui/AppDialog.tsx:47-54`, `:136-140`). `validateDialogSizes`
declares those sizes `int: true` (`persisted/domains.ts:139-150`), so `num`
rounds them and calls `r.note` — a repair, on every load, of a value the
application itself wrote one keystroke earlier. Together with defect 1 that is
a permanent "Dialog sizes" notice with nothing actually wrong.

### 3. The notice is an error, is not deactivatable, and overstates itself

`App.tsx:155-164` raises it with tone `"error"`. A clamped dialog width is not
an error, and the sentence — "Stored data was unusable and has been reset" —
describes total loss, which is what a quarantined unparseable blob is, not what
a rounded number is. There is no setting for it, so the only way out is to know
about `localStorage` and the key names.

The gate's own design notes say the notice exists because "silently resetting
somebody's settings is how an application earns the reputation of 'it forgets
things'". That is right. A notice that cries loss on every start for a rounded
pixel is how it earns the opposite reputation, and it trains the user to
ignore the one that matters.

## Required change

- Write the repaired value back once, per domain, after hydration: a repair
  that is not persisted is a repair that repeats forever. The quarantine copy
  is taken before the write, as it is today, so nothing is lost by it.
- Round dialog sizes where they are produced, so a value the application wrote
  itself is never read back as a repair.
- Separate the two things the notice conflates:
  - *reset* — the stored text could not be parsed, or came from a newer schema:
    data really is gone, and this stays an error,
  - *repaired* — values were clamped or dropped and the rest was kept: an
    informational notice with its own wording.
- Make the notice deactivatable: a setting (default on) next to the other
  persisted-state controls in Settings ▸ General ▸ Back up settings. Turning it
  off silences the toast only — the console diagnostics and the quarantine copy
  are unaffected, because they are what a support bundle is read from.

## Verification

- Vitest: a domain repaired at startup has the repaired value in
  `localStorage` afterwards, and a second startup from that text raises no
  notice.
- Vitest: an unparseable domain is reported as a reset, a clamped one as a
  repair, and a mixture reports the reset.
- Vitest: with the setting off, neither notice is raised, while the quarantine
  copy is still written.
- Vitest: `clamp` returns whole numbers for a fractional window width, and the
  value it persists survives `validateDialogSizes` unrepaired.
- `npm run check`.

## Implementation

The loop is broken at its source, and what is left is reported for what it is.

### The repair is written back

`gate.ts` gained a writer registry. Each persisted store registers one line
after it is created — `onRecovered("settings", () => useSettings.setState({}))`
— and `settleRecovered()` calls the writers of the domains that actually needed
recovering. `setState({})` merges nothing and notifies, which is exactly what
makes Zustand's `persist` save the current state; there is no supported "write
now" call, and a second storage adapter for the purpose would be a second thing
to keep correct. The quarantine copy is taken before this, as it always was, so
nothing is lost by the write.

App calls it once in the startup effect, after the first render: the stores are
hydrated by then, and a write during module load would land inside Zustand's own
rehydration. A writer that throws is logged and skipped — that domain simply
reports again next time, which is the honest outcome.

### The value being repaired stopped needing it

`AppDialog`'s `clamp` rounds. `window.innerWidth * 0.9` is fractional for every
odd window width, so the size the component itself persisted came back as a
repair on every load of a domain that was never damaged.

### Two events, told apart

`Recovery` is `"repaired"` or `"reset"`. `rehydrate` marks a repair,
`reportUnreadable` and `acceptVersion` mark a reset, and `mark` never lets a
repair downgrade a reset on the same domain. App raises one notice for the one
slot a pane has: the reset wording as an error when anything was lost, the
repaired wording as information otherwise. The German and English text for the
reset case also stopped claiming the data was "unusable" when what happened is
that it could not be read.

### Deactivatable

`reportPersistRecovery` (default on) sits with the other persisted-state
controls, in Settings ▸ General ▸ Back up settings, since that panel is the only
place the application talks about this data at all. It silences the toast only:
`settleRecovered` still runs, the console diagnostics are unchanged, and the
quarantined copy is still written — those are what a support bundle is read
from, and the switch is about noise, not about evidence.

### Bundle

The eager entry chunk had 0.8 kB of headroom, and this change needed more than
that. Rather than raise the budget, the `persist` namespace was split the way
the loading boundary already implies: the startup notice and the domain names
stay core, and the 22 keys that are only ever read inside the Settings dialog
moved to a `backup` namespace that registers itself when that lazy chunk loads.
The entry chunk is 322.4 kB — below where it stood before this ticket.

### Verification

- `gate.test.ts` ▸ "recovery levels": a repair and a reset are reported apart,
  and a reset is not downgraded by a later repair in either order.
- `gate.test.ts` ▸ "settleRecovered": only recovered domains are written back, a
  writer that throws does not stop the others, and nothing recovered writes
  nothing.
- `App.test.tsx` ▸ "reports a corrected value as a correction and writes it
  back": the real shell raises `persist.repaired` with tone `info`, and
  `hortcmd-dialog-sizes` afterwards holds the repaired value — which is what
  makes the next start silent. Rendering the actual application is the point:
  the write-back only works if the store that registered the writer is the one
  that hydrated.
- `App.test.tsx` ▸ "says nothing about recovered data once the notice is
  switched off": no notice, while the value is still written back and the
  quarantined copy is still there.
- `AppDialog.test.tsx` ▸ "clamp": whole pixels for a window width the 90 % bound
  cannot divide, and the result survives `validateDialogSizes` with no repair
  recorded — the two halves of the loop, asserted against each other.
- `npm run check` passed on macOS (arm64); 810 frontend tests, entry chunk
  322.4 kB against a 325 kB budget.
