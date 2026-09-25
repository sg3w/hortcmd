# TICKET-327: Classify text vs. binary on a UTF-8-safe buffer boundary

Discovered during a full-codebase review (2026-09-22).

## Problem

`looks_binary` in `src-tauri/src/commands/fs/filecompare.rs` (around lines
117-119, 240-242) runs `std::str::from_utf8(bytes)` on the buffer returned by
`read_capped`, which truncates at exactly `MAX_TEXT_BYTES` (4 MiB) regardless
of UTF-8 character boundaries.

A legitimate UTF-8 text file larger than 4 MiB whose byte at offset
4,194,304 happens to fall in the middle of a multi-byte character (an
accented letter, emoji, CJK character) makes `from_utf8` fail on the
truncated buffer, so `looks_binary` returns `true` and the file is
misclassified as binary — the user gets a hex dump instead of the text diff
they'd expect, purely because of where the truncation cut landed.

## Required change

- Classify on a buffer trimmed back to the last valid UTF-8 boundary (e.g.
  via `str::from_utf8`'s error `valid_up_to()`) before the NUL/UTF-8 binary
  check, or classify only on the untruncated prefix known to be complete.

## Verification

- Test: a >4MiB UTF-8 text file constructed so a multi-byte character straddles
  the `MAX_TEXT_BYTES` boundary; assert it's classified as text, not binary.
- `npm run check`.
