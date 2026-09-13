# TICKET-079: Apply the same prompt validation to Enter and the submit button

## Problem

Found while verifying ticket 249. `PromptDialog` disables the submit button for
whitespace-only input, but its Enter handler calls `submit()` without that guard.
An empty rename, pack or mkdir prompt therefore closes without doing anything.
The selection-pattern callback instead interprets an empty pattern as `*`.
Archive-password input also needs an explicit policy: whitespace can be a valid
password and must not be trimmed like a file name.

## Scope

Use one explicit validation policy for pointer and keyboard submission. Keep an
invalid prompt open with focus on its input. Preserve the original password bytes
when passwords are accepted. Review all prompt callers and retain any deliberately
supported empty-pattern behavior through an explicit option rather than an Enter
loophole.

## Acceptance

- Enter and clicking the button accept and reject the same values.
- Invalid submission does not close the dialog or call its callback.
- Tests cover empty input, whitespace, selection patterns and archive passwords.
- `npm run check` passes; make a local ticket-only commit, without pushing.

## Scheduling

Follow the requested sequence through 065 first. File-operation callbacks already
reject empty names, so this finding does not require interrupting that sequence.
