# TICKET-086: Report actionable system-trash failures

## Problem

Found during native macOS verification of ticket 114. A trash confirmation for
a disposable test file reached a Finder-automation permission prompt, then ended
with the localized generic message `Lese-/Schreibfehler`. The file remained intact.
The available observation does not establish whether permission denial, another
Finder failure, or a filesystem error caused that particular failure.

`src-tauri/src/errors/mod.rs` maps every `trash::Error` to `FS_IO` through
`from_library_error!`. Consequently, the UI cannot distinguish actionable system
trash failures even when the underlying error supplies a more specific cause.

## Requirements

- Inspect the supported trash library's structured error variants and preserve
  meaningful causes in the existing typed error model where possible.
- Distinguish permission failures, missing entries and other I/O failures when
  supported by reliable evidence; do not infer permission denial merely from the
  presence of an earlier OS prompt or from localized error-string matching.
- Provide localized guidance for any platform-specific failure that can be
  identified reliably, retaining the project's diagnostic redaction policy.
- Preserve per-item results and never fall back to permanent deletion.
- Cover the error mapping and UI behavior with meaningful regressions. Include
  a macOS smoke test for refused Finder automation when an environment is available.
- Run `npm run check` and create a local ticket-only commit without pushing.

## Scheduling

Follow the requested sequence through 065. The observed failure preserves the
source file and is displayed to the user; no data-loss workaround is required.
Successful native trash disposal remains an unverified part of ticket 114.

## Completion

Replaced blanket FS_IO conversion with evidence-based trash error mapping.
Added local entry metadata checks without following symlinks, localized Finder
and generic trash guidance, and regressions for per-item failures, retained files,
redaction and the German/English error dialog without permanent-delete fallback.
See `docs/trash-errors.md`; controlled native verification is tracked in 136.
Validation: targeted Rust/UI regressions and `npm run check`.

The measured bilingual-message growth (319.8 → 320.5 KiB entry) is documented in
Concept.md with a 321 KiB budget. Translation loading follow-up: ticket 137.
