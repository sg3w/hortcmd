# TICKET-126: Retain archive places in navigation history

## Problem

Archive navigation did not record history, and back/forward always loaded an
ordinary directory. Its position also changed before the destination loaded.

## Implementation

- Store each place as a path and optional archive container.
- Record normal and opposite-pane archive opening, inner navigation and exit.
- Reload back/forward destinations through the matching directory/archive command
  and commit history only with a successful, still-current response.
- Retain the destination index through password prompts and retry.
- Normalize legacy in-memory string entries to directory places on successful
  navigation. The saved workspace layout still stores only folder paths, so its
  existing schema and restart behavior remain compatible.

## Verification

Regression tests cover directory/archive/remote transitions, distinct containers,
forward-branch replacement, duplicate prevention, opposite-pane opening, parent
exit, failed directory and archive loads, late responses, password retry, legacy
strings, and saved workspace capture/restore.

Validation passed: `npm run check`, including the navigation and password
regression suites and the production bundle budget.
See [the navigation contract](../../docs/vfs-locations.md#pane-navigation-history).
