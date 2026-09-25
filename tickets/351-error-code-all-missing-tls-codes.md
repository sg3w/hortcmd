# TICKET-351: Add the missing TLS/certificate codes to code::ALL

Discovered during a full-codebase review (2026-09-22).

## Problem

Nine error codes that are actively constructed elsewhere in the backend —
`NET_TLS_PROTOCOL`, `NET_CERTIFICATE_EXPIRED`, `NET_CERTIFICATE_FUTURE`,
`NET_CERTIFICATE_HOSTNAME`, `NET_CERTIFICATE_REVOKED`,
`NET_CERTIFICATE_INVALID`, `NET_CERTIFICATE_CHANGED`, `NET_PLAINTEXT_BLOCKED`,
`NET_TLS` — are missing from `code::ALL` in `src-tauri/src/errors/mod.rs`
(definitions around lines 254-264, `ALL` around lines 295-347), even though
the doc comment directly above `ALL` states "The list is exhaustive so the
translation snapshot test can walk it."

The in-module test `codes_are_unique_and_namespaced` only walks `code::ALL`,
so it silently skips uniqueness/namespace checks for exactly the nine
TLS/certificate codes that matter most for security UX. The *frontend*
translation check (`npm run gen:errorcodes`) is unaffected — it regexes the
`pub const` declarations directly rather than using `ALL` — and translations
for all nine codes currently exist, so there's no live translation gap today.
But the Rust-side invariant `code::ALL` claims to provide (exhaustive,
deduped, namespaced) is currently false, and any future test or tool written
against the documented contract of `ALL` will silently miss these codes.

## Required change

- Add the nine missing constants to `code::ALL`, or replace the hand-kept
  array with a macro/build-script that derives it from the same `pub const`
  declarations `gen-error-codes.mjs` already scans, so the two can't drift
  again.

## Verification

- `cargo test codes_are_unique_and_namespaced` after the fix; confirm the
  nine codes are now covered.
- `npm run check`.
