# TICKET-125: Restore the index search dialog within the session

## Problem

Unmounting the index search panel discarded its selected scope, query filters and
results. Restoring this state also needs to respect locked or changed scopes and
responses from requests started by a previous dialog instance.

## Implementation

- Keep one in-memory Zustand snapshot for the current initial root, including
  creation preferences, query filters, the bounded result page and query status.
- Validate the selected scope through the backend before displaying cached results
  or errors. Invalidate results and the content query on scope selection, locking,
  removal, root/policy changes or loss of permitted offline access.
- Never save passwords or persist the snapshot to browser storage.
- Cancel queries on close or explicit stop; reject late query, creation and control
  responses after close, cancellation or a selection change.
- Revalidate the scope after a query completes and before showing its results.
- Restore without automatically submitting another query.

## Verification

The component regression suite covers reopening after validation, unchanged query
counts, locked/removed/changed scopes, query cancellation and late completions,
creation preferences without passwords, changing the initial root, restored query
errors, and locking followed by unlocking without reviving old results.

Validation passed: `npm run check`, including all 17 index panel regression tests,
Rust tests, strict Clippy, generated bindings, TypeScript, frontend tests and the
production build within its entry bundle budget.

See [the search index session contract](../../docs/search-index.md#search-dialog-session).
