# TICKET-080: Preserve whitespace in connection secrets

## Problem

Found during ticket 114's connection-dialog inspection. `ProfileForm.submit`
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

## Completion
- ProfileForm now sends `null` only for a truly empty field (untouched or cleared).
  Every nonempty value replaces the stored credential without trimming, including
  whitespace-only passwords and passphrases. The form does not offer credential
  deletion or replacement with an empty string. New profiles with an empty field
  do not write a credential.
- Added a German/English field description explaining retain/replace behavior;
  password and passphrase inputs expose it as their accessible description.
- Reviewed KeePass unlock, archive-password prompts, IPC/store forwarding and
  backend credential storage/authentication: no further secret trimming found.
- Actual dialog tests verify exact net_save_profile arguments for password, key
  and digest auth, untouched/cleared inputs and blank fields upon reopening.
- Validation: 18 ConnectionsDialog tests and `npm run check`.
