# TICKET-128: Display structured batch errors in the rename dialog

## Problem

Batch rename returns structured ItemError values. Joining those objects directly
rendered `[object Object]` instead of the failed filename and translated cause.

## Implementation

The dialog now uses the shared `describeItemErrors` formatter and limits the
formatted output to twelve lines. Empty or cancellation-only errors do not open
an empty confirmation. The existing rename payload and pane-refresh behavior
remain unchanged.

## Verification

Component tests cover partial success with multiple failures in German and
English, unchanged rename arguments and both-pane refresh, and empty or
cancellation-only results. Required gate: `npm run check`.

The separately discovered request-lifetime issue is recorded in
[TICKET-300](300-rename-request-lifecycle.done.md).
