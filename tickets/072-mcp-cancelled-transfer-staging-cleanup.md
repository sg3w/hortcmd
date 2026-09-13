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
