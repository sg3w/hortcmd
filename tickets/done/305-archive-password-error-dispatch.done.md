# TICKET-305: Preserve structured archive password errors

Discovered while implementing archive navigation history (ticket 126).
`loadArchive` converts a structured backend error to a string before passing it
to `isPasswordError`, which recognizes only structured error codes. Encrypted
archives therefore display a generic error instead of requesting a password.

- Dispatch password-required and password-wrong errors using their original code.
- Retain the structured error in the pane state.
- Test the password prompt, retry label, submitted password and stale response guard.
- Validation passed: `npm run check` (including four password regressions).

Implemented with four regression cases covering both error codes, password
submission, an obsolete prompt and a superseded response.
