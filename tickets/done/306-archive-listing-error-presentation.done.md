# TICKET-306: Present structured archive listing failures correctly

Discovered while fixing ticket 305: `loadArchive` converts a backend `AppError`
to `String(error)` for the general failure dialog. The resulting message can be
`[object Object]` instead of a useful translated explanation and correlation ID.

- Format the dialog through the shared structured error presentation helper.
- Preserve the pane's structured error and the previously displayed location.
- Test a structured non-password error and an unstructured transport failure.
- Verify password failures still use their dedicated prompt.
- Validation passed: `npm run check`, including localized known errors, unknown
  codes with a reference, transport failures and the password regression suite.
