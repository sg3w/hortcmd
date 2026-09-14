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
- Keep the Windows/Linux native matrix in TICKET-259 deferred separately.

## Acceptance

Repeated-separator resources can be read and transferred without changing their
identity, including during recovery; `npm run check` passes.
