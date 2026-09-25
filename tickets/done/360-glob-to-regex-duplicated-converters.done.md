# TICKET-360: Consolidate the two glob-to-regex converters in search

Discovered during a full-codebase review (2026-09-22).

## Problem

Two independent, inconsistent glob→regex converters exist in the same
feature area: `src-tauri/src/commands/fs/search.rs` lines 145-172
(`glob_to_regex`) vs. `src-tauri/src/commands/fs/search_index/mod.rs` lines
37-49 (`glob`).

`search.rs::glob_to_regex` supports multiple space/`;`-separated glob
patterns joined with `|` and translates char-by-char, while
`search_index::glob` (used by `query.rs` name/path filters and policy
`ignore` patterns) only supports a single pattern and builds the regex via
`regex::escape(...).replace("\\*", ".*").replace("\\?", ".")`. Same problem,
two different implementations with different feature sets living in the same
module family.

## Required change

- Consolidate into one shared glob-to-regex helper so both call sites get
  the same semantics (e.g. multi-pattern support, or lack thereof,
  consistently).

## Verification

- Existing search/search_index tests continue to pass after consolidation;
  add a test covering the previously-divergent behavior (multi-pattern glob)
  for whichever call site gains it.
- `npm run check`.

## Resolution (2026-09-25)

Live and indexed search now use one anchored glob converter with the same wildcard and multiple-pattern semantics. Indexed search retains its length and regex allocation limits. Regressions cover alternatives, literal metacharacters, case sensitivity and oversized patterns.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
