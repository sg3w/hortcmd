# TICKET-364: Share one is-link-mode helper across ZIP/7z preflight and extract

Discovered during a full-codebase review (2026-09-22).

## Problem

The unix-mode/windows-attributes "is this a symlink/special file" test
(`kind != 0 && kind != 0o100000 && kind != 0o040000`) in
`src-tauri/src/commands/fs/archive/extract.rs` is duplicated four times —
twice for ZIP (preflight and extract, lines 62-65 and 215) and twice for 7z
(preflight and extract, lines 89-93 and 348-352) — with slightly different
surface syntax each time.

Not a live bug today (the four copies currently agree), but it's exactly the
kind of security-relevant predicate where a future edit to one copy (e.g.
tightening the mask, or fixing a real bug found in one path) can silently
miss the other three, reintroducing a symlink-detection gap on one
format/phase without any test catching the divergence unless someone thinks
to update all four.

## Required change

- Extract a shared `fn is_link_mode(mode: u32) -> bool` (or two small helpers
  `zip_is_link(&ZipFile)`, `sevenz_is_link(&Entry)`) in `safety.rs` or
  `extract.rs` and call it from all four sites.

## Verification

- Existing archive extraction tests continue to pass unchanged after the
  refactor.
- `npm run check`.

## Resolution (2026-09-25)

ZIP preflight and both directory/file extraction checks now use one mode predicate; 7z preflight and extraction share one entry predicate built on it. Existing link, special-file and archive extraction tests pass without changing format behavior.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
