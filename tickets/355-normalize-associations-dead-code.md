# TICKET-355: Actually call normalizeAssociations during settings validation

Discovered during a full-codebase review (2026-09-22).

## Problem

`normalizeAssociations()` in `src/store/persisted/settings.ts` (associations
block lines 238-247, helper at lines 258-274) exists specifically to
lowercase/strip-dot legacy association keys on rehydration ("A stored `.LOG`
came from before that, or from an import"), but it is never called anywhere
in the codebase — `validateSettings`'s `associations` field is built via
`record()`, which preserves keys verbatim.

A settings file imported via `readImport` (`src/store/persisted/
transfer.ts`) or an older/hand-edited `localStorage` payload containing an
unnormalized key (e.g. `.LOG`) passes validation untouched.
`programForExtension()` and `setAssociation()` always look up/write via
`normalizeExt(ext)` (lowercase, no dot), so the stored `.LOG` entry can never
match a lookup for `log` — the association silently becomes permanent dead
weight, appearing lost to the user even though nothing flagged a repair.

## Required change

- Call `normalizeAssociations` on the `associations` result inside
  `validateSettings` (routing collisions/changes through the existing
  `Repairs` counter so the "repaired" notice fires), matching what its own
  doc comment promises.

## Verification

- Test: import/rehydrate a settings payload with an unnormalized association
  key (e.g. `.LOG`); assert it's normalized and matches `programForExtension`
  lookups afterward.
- `npm run check`.
