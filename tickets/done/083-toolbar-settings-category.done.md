# TICKET-083: Preserve the settings category when opened from the toolbar

## Problem

Found in the browser while verifying ticket 249 at 640 × 360. Clicking the
settings toolbar button opens an empty form. `ToolButton` forwards React's click
event to `openSettings`, whose optional argument is a settings category. The event
is stored as that category; no page matches it. DOM inspection confirmed every
category was unselected and the panel referred to `[object Object]`.

## Fix

Call `openSettings()` from a zero-argument wrapper. Preserve the current category
when the toolbar opens the dialog. Interpose immediately because this entry point
otherwise opens settings without a usable form.

## Verification

Two regressions failed before the fix and now pass for the general/view category.
The browser toolbar entry point now opens General with all six field groups at
640 × 360; the dialog bounds remain inside the viewport. Full `npm run check`
passed on Node 20.19.0: 387 Rust tests (25 ignored), 457 frontend tests, formatting,
Clippy, bindings, TypeScript, documentation and UI-text gates, build and budgets.
Local commit only; no push.
