# TICKET-095: Use provider components throughout streaming operations

## Problem

TICKET-055 can represent and compare repeated remote path separators losslessly,
but existing streaming interfaces still use decoded `Path`/`PathBuf` adapters.
For example, deriving the parent of `/a//b` can discard the empty component.
On a WebDAV server where that component distinguishes resources, staging could
then select the wrong parent. VFS resolution now refuses repeated separators
before any provider operation; it does not silently normalize them.

## Requirements

- Carry the typed provider path through transfer planning, staging, rollback,
  provider traversal and publication; convert only at native-local boundaries.
- Preserve empty components in parent/join/basename operations and provider calls.
- Validate with a fake provider that distinguishes `/a/b` from `/a//b`, including
  failed publication/rollback and MCP containment.
- Remove the conservative `provider-path-adapter` refusal only after those tests
  demonstrate the exact destination object is preserved.
- Keep the Windows/Linux native matrix in TICKET-123 deferred separately.

## Acceptance

Repeated-separator resources can be read and transferred without changing their
identity, including during recovery; `npm run check` passes.

## Implementation

- Streaming provider APIs now use an explicit local/remote path union. Native
  conversion is limited to local I/O; remote paths retain their components.
- Transfer preflight, directory traversal, temporary files, backup publication,
  rollback and recovery lookup preserve repeated separators.
- FTP/SFTP/WebDAV retain remote parents during listing and recursive directory
  operations. SMB refuses ambiguous repeated separators at its own boundary.
- Frontend parent navigation and the shared location corpus use the same rules.
- Regression tests distinguish normalized aliases from exact resources, covering
  verified copy, failed publication/rollback, journal reopening and MCP scope.
- Cross-store rename validation found during review is tracked in TICKET-098.
- Live protocol and Windows/Linux verification remain deferred to TICKET-123.

## Validation

`npm run check` passed: formatting, strict Clippy, 539 Rust tests (28 explicitly
ignored), unchanged generated bindings, repository checks, TypeScript, 678
frontend tests, production build and bundle budget (entry 320.6 KiB / 321 KiB).
The remote regression suite also covers cancellation and reconstruction of typed
paths from the persisted transfer plan before execution.
