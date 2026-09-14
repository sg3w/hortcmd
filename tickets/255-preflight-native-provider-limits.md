# TICKET-255: Validate native Windows/Linux preflight constraints

## Goal

Implement and validate native provider limits outside the current macOS development
batch. The user explicitly deferred Windows/Linux-specific work.

## Requirements

- Query stable destination identities, writable state, per-volume case sensitivity,
  name/path limits and available quotas with native Windows/Linux adapters.
- Exercise reserved names, sparse files, hard links, cross-volume moves and changing
  capacity in real native filesystems.
- Keep unsupported query results explicitly unknown and preserve commit checks.

## Acceptance Criteria

- [ ] Windows native constraint and revalidation matrix passes.
- [ ] Linux native constraint and revalidation matrix passes.
- [ ] Documentation describes provider-specific known and unknown constraints.

## Definition of Done

Implement on a dedicated ticket branch, pass `npm run check` and native validation,
then merge locally. Builds on [TICKET-049](done/049-transfer-preflight-and-resource-estimation.done.md).
