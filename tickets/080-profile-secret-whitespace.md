# TICKET-080: Preserve whitespace in connection secrets

## Problem

Found during ticket 249's connection-dialog inspection. `ProfileForm.submit`
trims passwords and key passphrases before calling `save`. Credentials with
leading or trailing spaces therefore differ from what the user entered;
whitespace-only secrets become `null` (keep the old credential).

## Required change

Preserve secret bytes on submission. Distinguish an untouched/empty secret from
an intentional credential change, and document the retain/replace behavior.
Review the credential forms for the same issue. Do not trim secrets in backend
storage or authentication code.

## Acceptance

- A password/passphrase containing leading or trailing spaces reaches the
  credential store unchanged.
- The supported empty/unchanged-secret behavior is explicit and tested.
- Regression tests exercise the actual dialog and Tauri command arguments.
- `npm run check` passes; commit only this ticket locally, without pushing.

## Scheduling

After the requested sequence through 065; existing stored credentials are not
rewritten merely by opening the dialog. This does not block accessibility work.
