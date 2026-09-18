# TICKET-300: Handle batch-rename request failures and prevent duplicate submissions

## Finding

Found while implementing ticket 128. In `RenameDialog.tsx`, `apply` awaits
`renameBatch` without a rejection handler or an in-flight guard. The Apply button
stays enabled while the request runs. A transport/backend rejection escapes the
click handler, and repeated clicks can submit the same rename plan concurrently.
The same callback can close a newly reopened dialog after an older request ends.

## Requirements

- Allow only one submission per dialog operation, including rapid repeated clicks.
- Present rejected requests through the shared structured error formatter.
- Restore usable controls after failure and preserve the editable rename plan.
- Ignore stale UI completion after close/reopen while refreshing affected panes
  for operations that actually completed.
- Keep partial-success item errors and the existing preview/conflict checks.

## Acceptance criteria

- Deferred-request tests prove repeated clicks start only one backend call.
- A rejected request produces a visible localized error without an unhandled rejection.
- Closing/reopening prevents the old request from closing the new dialog.
- `npm run check` passes.


## Implementation evidence

A synchronous in-flight guard prevents duplicate requests; controls remain disabled
until completion. Rejections use structured localized errors in the existing
rename dialog and preserve the plan. A dialog revision identifies each opening,
so late completion cannot close or overwrite a newer dialog. Completed filesystem
changes still refresh affected panes.

`npm run check` passed. Deferred component tests cover duplicate clicks, rejection
and retry, completion after reopen, and a stale rejection after reopen.
