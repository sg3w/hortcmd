# TICKET-366: Route plugin UI errors through describeErrorFull

Discovered during a full-codebase review (2026-09-22).

## Problem

The entire plugins UI (`src/features/plugins/pluginsStore.ts:40`,
`PluginManagement.tsx:27`, `PluginCommandBar.tsx:19`, `PluginTabs.tsx:21`,
`PluginFrame.tsx:40`) displays errors via `String(error)`/`String(cause)`
instead of the app's `describeError`/`describeErrorFull` used consistently in
`net/`, `panel/`, `commandLine/`, and even `settings/McpForm.tsx`/
`McpRuntime.tsx`.

Any plugin install/activate/deactivate/launch/open failure is shown
unlocalized (backend text, not the i18n dictionary) regardless of the user's
language setting, and without the retry/reference-id affordances every other
error in the app gets. Root cause: the backend's plugin commands
(`src-tauri/src/commands/plugins.rs`) return `Result<T, String>` rather than
the typed `AppError` the rest of the backend uses, so `describeErrorFull`
would degrade to the same raw string here — the deeper fix is on the backend
side (a larger, separate effort), but the frontend inconsistency is real and
worth fixing now.

## Required change

- At minimum, route these five call sites through `describeErrorFull` for
  consistent formatting/fallback behavior.
- Note for a future ticket: ideally have the plugin commands return
  `AppError` like the rest of the backend, which would also improve this
  further.

## Verification

- Test: a plugin install/activate failure; assert the displayed message goes
  through `describeErrorFull` rather than a raw `String(error)`.
- `npm run check`.

## Resolution (2026-09-26)

All five plugin UI error sites (`pluginsStore.refresh`, `PluginManagement.run`,
`PluginCommandBar`, `PluginTabs`, `PluginFrame`'s request bridge) now format
failures with `describeErrorFull` instead of `String(...)`, so typed backend
errors render in the user's language with the reference-id fallback, and the
still-untyped `Result<_, String>` plugin commands degrade to the same raw text
as before. A `PluginManagement` test installs a plugin whose backend call
rejects with a typed `fs.permission` error and asserts the alert shows the
localized sentence and not the diagnostics-only `detail`.

Validation: `npm run check` passed on macOS.
