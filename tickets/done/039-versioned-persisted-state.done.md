# TICKET-039: Version, validate, and migrate persisted frontend state

## Goal

Ensure settings, sessions, histories, dialog sizes, and window state remain safe
and usable across application upgrades, schema changes, corruption, and invalid
values.

## Background

Several Zustand stores persist directly to localStorage, including settings,
session tabs, history, and dialog sizes. Window geometry is stored separately.
The current stores do not declare schema versions or migrations. Some setters
clamp new values, but rehydrated data can bypass setter validation, retain removed
fields, reference deleted programs, point at unavailable remote profiles, or use
invalid enum values. TICKET-029 adds a default-enabled transfer-safety setting,
which makes explicit migration behavior a data-safety requirement.

## Requirements

- Inventory every persisted key, schema, owner, retention rule, and sensitivity.
- Define an explicit monotonically increasing version for each persisted domain or
  one documented application-state envelope.
- Add deterministic migrations from every released schema version to the current
  version, including the absent-value-to-enabled rule for failure-safe transfers.
- Validate rehydrated data at runtime before it reaches application stores.
- Clamp numeric values, reject invalid enums and colors, normalize paths and file
  extensions, remove dangling program associations, and cap unbounded list sizes.
- Treat malformed or future-version data as recoverable: quarantine or export the
  original value, restore safe defaults for the affected domain, and notify the
  user once without preventing startup.
- Keep secrets out of localStorage. Connection-profile references may be persisted,
  but passwords, passphrases, tokens, and private-key contents may not.
- Handle unavailable local paths, remote profiles, displays, and window geometry
  without repeated startup errors.
- Add an in-app reset/export/import path for non-secret preferences with schema
  validation and a preview of destructive reset scope.
- Make migrations idempotent and never silently downgrade a newer schema.

## Acceptance Criteria

- [x] Every persisted value has a documented schema version and runtime validator.
- [x] Upgrading from all released schemas preserves valid user preferences.
- [x] Missing `useFailureSafeTransfers` migrates to `true` as required by
      TICKET-029.
- [x] Corrupt data cannot prevent the two-pane shell from opening.
- [x] Invalid references and out-of-range values resolve predictably with a single
      localized notification.
- [x] No secret is included in persisted state, exports, or migration diagnostics.
- [x] Import rejects incompatible or tampered content without partial application.

## Tests

- Add fixture-based tests for every historical schema version and expected current
  output.
- Property-test validators with missing, extra, wrong-type, oversized, and deeply
  nested input.
- Test corrupt JSON, future versions, interrupted migrations, and repeated runs.
- Test unavailable monitors, local roots, connection profiles, editors, and tabs.
- Test export/import round trips and verify secret-like fields are rejected or
  removed.
- Test the failure-safe transfer default for fresh, legacy, explicit-on, and
  explicit-off states.

## Definition of Done

- Persisted frontend domains are versioned, validated, bounded, and migratable.
- All released schemas have regression fixtures.
- Recovery from invalid data is safe, localized, and does not block startup.
- Preference import/export excludes secrets and validates atomically.
- Schema ownership and compatibility policy are documented.
- Rename this file to `039-versioned-persisted-state.done.md`.

## Implementation

**Rehydration goes around every setter.** That is the whole problem in one
sentence. `setPaneSplit` clamps to 0.15…0.85 and has since the divider existed;
a stored `paneSplit: 400` was written straight into the store on the next start,
leaving one pane invisible with no gesture to get it back and no way to find the
key that caused it. The same gap let an association point at a deleted program,
a colour override hold something that is not a colour, and a write interrupted
by a crash mean "your settings are gone", silently.

### One policy, five domains

`src/lib/schema.ts` holds the pieces — `bool`, `num`, `str`, `oneOf`, `color`,
`list`, `record`, `unique` — under two rules. A validator never throws and never
returns nothing, so a caller cannot forget a case; and every repair is
*counted*, because silently fixing user data is how a setting disappears without
anyone knowing why.

What happens to a bad value depends on what kind of bad it is:

- **out of range → clamped.** A stored 400 means the user once dragged the
  divider, and 0.85 is closer to what they wanted than the default is.
- **wrong type → default.** Nobody chose `"wide"`.
- **no identity → dropped.** A favourite without a path cannot be shown or
  removed; there is nothing to repair it *to*.
- **dangling reference → dropped.** An association pointing at a program that no
  longer exists opens nothing and is invisible in the settings.
- **unbounded → capped.** 200 favourites, 100 programs, 20 history entries,
  4096-character paths.

`store/persisted/` applies it: `settings.ts` for the big one, `domains.ts` for
session, history and dialog sizes, `gate.ts` for the policy they share,
`storage.ts` for the adapter, `transfer.ts` for export and import. The persisted
settings *shape* moved to `store/settingsSchema.ts` so the validator can state
it without importing the store — which would be a cycle, and which
`check:boundaries` would rightly refuse.

### Three decisions worth the words

**The original is kept.** Repairing broken data and writing the result back
destroys the evidence along with the data. The raw text is copied to a
quarantine key first — the last five, then they rotate — and a parse failure
goes the same way. Zustand's own adapter returns `null` there and says nothing,
which is the right recovery and the wrong amount of silence.

**The user is told once.** Rehydration happens while the modules load, long
before anything can render, so the recovered domains are collected and a single
notice names them after the first paint. Never a modal, and never four notices
for one bad write.

**A newer schema is refused, not read.** Reading version 9 with the version-1
validator would drop whatever 9 added and then write the truncated result back,
downgrading data a newer installation still needs. The check lives in each
store's `migrate`, which is the only place Zustand hands over the version that
was actually on disk.

### The one behavioural migration, and a correction

TICKET-029 specified failure-safe transfers as the default — "for new *and*
existing installations … when the stored value is absent", with "Safe mode is
the default" in its Definition of Done — and its acceptance criterion was ticked.
It shipped with `safeTransfers: false`. So a stored `false` from before this
version is not a decision; it is the wrong default, recorded. v0 → v1 turns it
on, and the store's own default is now `true` as well, because a fresh
installation never runs a migration and the protection must not depend on
whether the user happened to run an older version first. The File-operations
panel already shows a standing amber warning while the setting is off, so anyone
who really wants direct writes turns it back off and sees why.

### Preferences travel, secrets do not

Settings → General gains Export, Import and Reset. Export writes one envelope
naming the application, the kind and the schema version; import reads it back
through the *same* migration and validation the store uses, because a file
somebody sent you is exactly as trustworthy as one on disk. Session, history,
dialog sizes and window position stay out: they describe one machine's last
state, not a preference.

A file is **refused** — before anything is touched — when it is not ours, when
it comes from a newer schema, or when it carries a field whose *name* suggests a
secret. Refused rather than stripped: quietly removing a password would hide
from the person importing that somebody put one in a preferences file. There are
no secrets to export in the first place (passwords live in the OS keychain and
profiles in the backend's config), which is exactly why the check is cheap to
keep. Reset lists the five domains it clears *and* the things it does not —
connection profiles, saved passwords, the files themselves — because "reset
settings" is ambiguous enough that somebody will fear for their connections.

### Window geometry

Its own failure mode: the display it was saved on may be gone. Every number in
the box is plausible and the window is simply invisible, with no way back but a
key nobody knows about. It is now bounded, versioned, checked against the
available monitors, and re-centred when it lands on none — keeping the size the
user chose, and saying so once at `info` level rather than looking like an error
on every start.

## Verified

`npm run check` passes end to end: `cargo fmt --check`, Clippy with
`-D warnings`, 278 Rust tests, current bindings and error codes, no import
cycles, `check:docs`, `tsc --noEmit`, **366 frontend tests** (63 new), the
production build, and the bundle budget (entry chunk 249.0 kB of 320 kB).

**Fixtures and hostile input** (`store/persisted/persisted.test.ts`, 36 tests): a
complete v0 settings object asserted field by field through the migration; the
failure-safe rule for fresh, absent, legacy, explicit-on and explicit-off;
migration idempotence; the validator's output as a fixed point of itself; values
of entirely the wrong shape (`null`, `42`, `[]`, a string); clamping for the
pane split and every transfer number; unknown enums; colours that are not
colours; entries with no identity; dangling associations; caps; deduplication;
unknown fields; a 100 000-character string; and the rule that an *absent* field
is not a repair, so an upgrade never looks like data loss.

**The recovery path** (`store/persisted/gate.test.ts`, 15 tests): a valid value
passes untouched; a repair quarantines the original and leaves the stored key
intact; validation runs even when the version already matched; a newer schema is
refused with the stored text preserved; recovered domains are reported once in a
stable order; the quarantine rotates at five; unparsable text is a recovery
rather than silence; and storage that throws on read *or* write — a private
window, a full quota — neither crashes nor raises a false alarm.

**Window geometry** (`lib/windowState.test.ts`, 12 tests): v0 and v1 boxes,
truncated JSON, a box with no size, clamping at both ends, negative coordinates
for a monitor to the left, a newer version refused, and `isOnAScreen` against
one monitor, two monitors, an unplugged one, a ten-pixel sliver, and the
no-information case.

**In the browser**, with damaged state planted by hand and the app reloaded:
`theme: "solarized"` → `dark`, `paneSplit: 400` → `0.85`, `bufferSizeKb:
9999999` → `16384`, a favourite without a path dropped, the association `txt →
"gone"` removed, `safeTransfers` migrated to `true`, the version written as `1`,
and the originals of both damaged keys under `hortcmd-quarantine-*`. The notice
read *"Gespeicherte Daten waren unbrauchbar und wurden zurückgesetzt:
Einstellungen, Verlauf"* — naming exactly the two domains that were damaged, and
gone again after its eight seconds. The two-pane shell opened normally
throughout. The reset confirmation was opened and lists its five domains and the
three things it leaves alone.

**Unavailable paths** were already handled and were re-read rather than
re-implemented: `restoreSession` skips a folder that no longer lists, falls back
to home when a side has nothing left, and clamps the active index — no repeated
startup error, which is what the ticket asks for.

Two things are worth stating plainly. The *session* is the one domain whose
schema did not change in this ticket; its version exists so the next change has
a number to migrate from, which is the point of starting the chain before it is
needed. And the failure-safe migration deliberately overrides a stored `false`
for every v0 installation — the alternative is leaving people on a default the
ticket that introduced it called a bug, and the change is visible the moment
they open the settings.
