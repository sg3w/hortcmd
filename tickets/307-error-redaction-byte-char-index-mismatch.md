# TICKET-307: Fix byte/char index mismatch in error-message redaction

Discovered during a full-codebase review (2026-09-22).

## Problem

`redact_assignments` in `src-tauri/src/errors/mod.rs` (around line 523) computes
match/scan positions as **byte** offsets (via `.find()` on a `String`) but then
indexes a `Vec<char>` with them. Any multi-byte UTF-8 character earlier in the
same string silently misaligns the scan, and the secret is never redacted.

Verified with a standalone repro of the function:
`redact_assignments("/Users/jörg/Dokumente: password=hunter2")` returns the
string **unchanged** — `hunter2` survives in full. This app is bilingual
(German UI, per `menu.rs`) and routinely handles non-ASCII paths/hostnames
(the repo's own test fixture uses `"/Users/jörg/…"`), so any provider error or
detail string combining a `password=`/`token=`/`apikey=` assignment with a
preceding umlaut, accented character, CJK, or emoji anywhere earlier in the
string leaks the raw credential into `AppError.detail`/`.params` — the exact
outcome CONTRIBUTING.md's "Security-sensitive changes" rule forbids.

This is the highest-priority finding from the review: a silent, no-panic
redaction bypass, not a crash.

## Required change

- Rework `redact_assignments` to operate in one coordinate space throughout —
  either use `char_indices()` end to end (as `redact_bearer` already does
  correctly), or work on byte slices consistently instead of mixing a
  `Vec<char>` with byte offsets from `.find()`.
- Add a redaction test that combines a non-ASCII character with a
  `password=`/`token=`/`apikey=` assignment in the same string — no current
  test exercises that combination.

## Verification

- New test: `redact_assignments` on a string containing an umlaut/accented
  character before a `password=`/`token=` assignment must strip the secret.
- `npm run check`.
