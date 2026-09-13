# TICKET-082: Preserve a file picker's follow-up request

## Problem

Found while implementing ticket 249. `FileBrowserDialog.pick` invokes
`request.onPick(full)` and then clears the file-browser store. A callback that
opens another file-browser request has that new request cleared immediately.
This is the same ordering issue fixed for confirmation dialogs in ticket 077.

## Requirements

- Capture the callback, close the current request, then invoke the callback.
- Test a first selection that opens a second picker and completes both callbacks.
- Keep focus in the second picker until it closes, then return to the workspace.
- Run `npm run check`; create a local ticket-only commit, without pushing.

## Scheduling

After the requested sequence through 065. The ordinary one-picker workflows
remain usable; this is a follow-up for chained requests.
