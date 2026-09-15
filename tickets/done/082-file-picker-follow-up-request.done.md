# TICKET-082: Preserve a file picker's follow-up request

## Problem

Found while implementing ticket 114. `FileBrowserDialog.pick` invokes
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

## Completion
- Capture onPick, close the current request, then invoke the callback. A picker
  opened by the callback remains active and initializes its own directory.
- Regression tests complete a first selection by pointer and the second by
  keyboard, assert both callback paths, and cover cancelling the second picker.
- Focus remains inside the second picker and returns to the invoking workspace
  control only after the chain closes; the existing focus-return hook suffices.
- Validation: six FileBrowserDialog tests and `npm run check`.
