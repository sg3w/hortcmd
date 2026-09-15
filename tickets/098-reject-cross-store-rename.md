# TICKET-098: Reject cross-store rename destinations

## Problem

While implementing TICKET-095, `commands/fs/file.rs::rename_entry` was found to
resolve both addresses but discard the destination provider. A direct command
call with two different remote stores can therefore rename a source-store entry
to the destination's path inside the wrong store. The normal rename dialog uses
the current folder, so this is a backend validation gap rather than a required
change to the dialog.

## Requirements

- Compare canonical store identities before existence checks, undo preparation
  or mutation; refuse cross-store rename with a structured error.
- Preserve the existing no-overwrite behavior for same-store renames.
- Test distinct providers with identical path strings and prove neither store
  changes on refusal; also cover local/remote pairs.

## Acceptance

A rename never interprets a destination belonging to another store as a path in
the source store. `npm run check` passes.
