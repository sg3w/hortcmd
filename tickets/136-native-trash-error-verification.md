# TICKET-136: Verify native trash errors and refused Finder automation

## Context

Ticket 086 preserves structured causes and reports Finder failures without
classifying osascript process exit codes as permission errors. Mapping and UI
regressions use controlled failures; a signed-app test with deliberately refused
Finder automation remains to be performed.

## Verification

- In a disposable macOS user/test-app setup, create a disposable file and refuse
  hortcmd's Finder automation request. Invoke F8 and confirm trash disposal.
- Verify the localized Finder guidance, absence of raw native diagnostics, and
  that the file still exists. Verify no permanent-delete retry was issued.
- Enable automation for that test app and verify successful disposal and Finder
  restoration separately. Record macOS/app identity, permission state and results.
- Do not reset or change the user's existing TCC permissions as an automated test.
- Later on Linux verify structured FileSystem permission/not-found causes; on
  Windows inspect structured HRESULT causes and preserve those that can be mapped
  reliably. Keep opaque errors distinct from evidence of permission denial.
