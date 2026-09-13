# TICKET-225: Contain archive extraction and limit hostile input

Originally ticket 046. Implementation is present; only the final platform/security
audit remains. Deferred by user instruction to keep the current batch focused on
bug fixes and new features. Historical implementation references retain 046.

## Goal

Ensure archives cannot write outside the selected destination, overwrite files
through links, exhaust local resources unexpectedly, or leave misleading partial
output after cancellation or failure.

## Background

ZIP, tar, tar.gz, tar.xz, and 7z entries are currently converted to relative
strings and joined directly to the chosen destination in
`src-tauri/src/commands/fs/archive.rs`. There is no shared containment validator
for absolute paths, `..` components, platform prefixes, mixed separators, Unicode
edge cases, or link-like archive entries. Extraction also trusts declared sizes
and writes directly to final paths. A crafted archive can therefore create path
traversal, resource-exhaustion, and partial-output risks in a privileged desktop
file manager.

This ticket applies the staging principles from TICKET-029 to extraction while
remaining independently testable for every supported archive format.

## Requirements

- Introduce one format-independent archive-entry validator before any directory or
  file is created.
- Reject absolute paths, Windows drive or UNC prefixes, parent traversal, empty or
  ambiguous normalized names, NUL bytes, alternate separators, and paths whose
  normalized target escapes the selected extraction root.
- Treat symlinks, hard links, reparse points, and format-specific link records as
  unsupported by default. If support is later enabled, prove both the link and
  every write through it remain inside the extraction root.
- Prevent pre-existing destination symlinks or directory substitutions from
  redirecting writes outside the root; use handle-relative or equivalent safe APIs
  where supported and fail closed elsewhere.
- Define configurable limits for entry count, total expanded bytes, per-entry size,
  path depth, path length, and compression ratio.
- Treat missing, contradictory, or overflowing size metadata as untrusted and
  enforce limits against bytes actually written.
- Detect duplicate normalized paths, file/directory conflicts, case-folding
  collisions, and platform-reserved names before commit.
- Preflight the complete archive when the format permits. For streaming formats,
  stop at the first unsafe entry and explain whether staged output was removed.
- Extract into a private staging location and publish only validated completed
  output. Preserve existing destinations until commit succeeds.
- Remove staging output after cancellation or failure and report cleanup failures
  through the structured errors from TICKET-036.
- Apply secure permissions to created staging directories and never log archive
  passwords or extracted file content.

## Acceptance Criteria

- [ ] No supported archive format can create or modify a path outside the chosen
      extraction destination.
- [ ] Absolute, traversal, mixed-separator, link, duplicate, and reserved-name
      entries fail with stable, localized errors.
- [ ] Expanded-size, entry-count, depth, and compression-ratio limits are enforced
      using overflow-safe arithmetic.
- [ ] Existing destinations remain intact until the staged extraction commits.
- [ ] Cancellation and failure leave no unreported partial output.
- [ ] Safe ordinary archives retain filenames, directory structure, and content.
- [ ] Security behavior is consistent on Windows, macOS, and Linux.

## Tests

- Maintain malicious fixtures for ZIP, tar variants, and 7z with `../`, absolute,
  drive-prefixed, UNC, mixed-separator, duplicate, case-colliding, and link entries.
- Test a pre-existing symlink inside the destination and a concurrent directory-to-
  symlink substitution where the platform permits controlled race testing.
- Test zip bombs, forged sizes, integer-boundary values, deep trees, long names,
  excessive entries, sparse-like content, and cancellation at every write phase.
- Verify destination containment by snapshotting parent and sibling directories
  before and after every malicious fixture.
- Test commit collisions and cleanup failures without damaging the old destination.
- Run the archive corpus on every supported operating system and under sanitizers
  or equivalent dynamic checks where available.

## Definition of Done

- All archive writes pass through one audited containment and resource-limit layer.
- Extraction uses failure-safe staging and commit semantics.
- A versioned malicious archive corpus prevents regressions across every format.
- Limits and unsupported entry types are documented in user and developer docs.
- Security review confirms no path is derived from archive metadata without
  validation.
- Rename this file to `225-safe-archive-extraction.done.md`.

## Implementation evidence (2026-09-13)

The validator, bounded writer, staging/rollback service and generated malicious
archive corpus are implemented in the archive submodules. The application
command is a thin adapter to the shared extraction service. Configuration and
recovery behavior are documented in [archive extraction](../docs/archive-extraction.md).

Local verification covers all five formats, cancellation, unchanged prior output,
Unicode/platform names, limits, a substituted destination symlink and a forced
publication failure with rollback. The Windows DACL helper was cross-compiled
with Rust 1.85.0. Native Windows/Linux execution remains to be verified. The previous MSRV
blocker was resolved in TICKET-068. Decoder-internal allocations are tracked by
TICKET-067 and are not bounded by the output-byte counter.

This ticket stays open for its final cross-platform/security audit.
