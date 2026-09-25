# TICKET-336: Fix fillParams re-substituting param values as placeholders

Discovered during a full-codebase review (2026-09-22).

## Problem

`fillParams` in `src/lib/appError.ts` (around lines 66-74), reached via
`src/store/errorText.ts`'s `describeItemError` (lines 88-94), substitutes
placeholders one param at a time via `split`/`join` folded over the *whole
accumulating string*, so a param value that itself looks like `{otherKey}`
gets re-substituted by a later iteration.

A batch operation fails on a file literally named `{message}.txt` (curly
braces are legal on POSIX and Windows). `describeItemError` calls
`fillParams("{item}: {message}", { item: "{message}.txt", message: "No
permission" })`. The `item` pass turns the text into `"{message}.txt:
{message}"`; the `message` pass then replaces **both** occurrences of
`{message}`, producing `"No permission.txt: No permission"` — the displayed
file name is silently corrupted into the error text. Any param whose value
can contain the literal substring `{otherParamKey}` (file/host/profile names
are all user-controlled) triggers this.

Note: this is a second, independent template-substitution bug alongside
ticket 307's backend redaction bug — both live in the error-display
pipeline but are unrelated fixes.

## Required change

- Do a single pass, e.g. one
  `replace(/\{(\w+)\}/g, (m, k) => params[k] ?? m)` over the original
  template instead of chaining `split`/`join` calls, so an inserted value is
  never rescanned for further placeholders.

## Verification

- Test: `fillParams` with a param value containing another param's
  placeholder syntax (e.g. a filename literally named `{message}.txt`);
  assert the output is not corrupted.
- `npm run check`.

## Resolution (2026-09-25)

Error templates substitute only placeholders in the original text, so inserted filenames and values are never scanned again. Tests cover placeholder-shaped filenames in either parameter order, repeated placeholders, literal replacement syntax, missing and empty values, nonword keys, and inherited property names.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
