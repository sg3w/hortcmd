# TICKET-072: Clean up MCP staging files after cancellation

## Finding
While verifying 041, inspection found that `ScopedProvider::check` rejects every
operation once a request is cancelled. This also rejects the shared transfer
engine's removal of its temporary staging file. The original destination and
move source remain protected; a reported hidden partial staging file can remain.

## Scope
- Permit cleanup only of staging entries owned by that transfer, without granting
  deletion of arbitrary paths or bypassing a revoked root/write permission.
- Keep cancellation checks on source deletion and destination publication.
- Test copy and move cancellation, scope revocation, and unrelated path refusal.
- Include staging leftovers in the user-visible result if safe cleanup fails.

## Priority
Follow-up after the requested sequence through 065. No publication of partial
files or deletion of the source is needed to address this cleanup issue.

## Completion
- Added an explicit cleanup path for the transfer's registered staging root after
  readers and writers have been released. A successful creation by the destination
  adapter is required; a matching filename prefix never grants permission.
- Live enabled state, write rights, allowed roots and local symlink checks remain
  enforced. Ordinary rename/remove operations still reject cancellation.
- Failed cleanup is included as its own error item with the staging address.
- Regression tests cover file/tree copy and move, cancellation before writing and
  before publication, root/write/server revocation, unrelated paths, pre-existing
  entries and unconfirmed creation.
- Validation: targeted MCP transfer tests and `npm run check`.
- Follow-up TICKET-270 records pre-existing provider allocation/replacement races.
