# TICKET-081: Settle search preflight cancellation and rejected commands

## Problem

Found during ticket 114. `SearchDialog.runSearch` awaits capability preflight
before starting the command, but does not recheck that its run is still current
after the await. Stop/close can therefore cancel a token before its native search
has started, and that search may subsequently be dispatched. The search promise
also has `finally` but no `catch`; a rejected command has no explicit error state
in the dialog.

## Requirements

- Recheck run ownership after asynchronous preflight and before dispatch.
- Closing/stopping during preflight must not start a later search.
- A stale preflight or summary must not overwrite a newer run.
- Show and announce a localized structured error on command rejection; distinguish
  that outcome from a completed search with no results.
- Test deferred capability answers, stop/close/new-run races and IPC rejection.
- Run `npm run check` and create a local ticket-only commit; do not push.

## Scheduling

Interpose before closing ticket 114: rejected searches must leave an accessible
error state, and cancelled preflight must not dispatch a later background scan.

## Completed verification

Run ownership is invalidated on stop/close/unmount, preflight is rechecked before
dispatch, and only a dispatched native token is cancelled. Stale batches,
summaries and cancellation failures cannot overwrite a replacement run. Rejected
searches expose localized structured error text in an alert and do not display
the empty-result message.

Six regressions cover stop/close during preflight, replacement preflight, a late
native summary, and German/English rejection messages. Full `npm run check`
passed: 387 Rust tests (25 ignored), 465 frontend tests, all formatting, Clippy,
bindings, TypeScript, documentation/UI-text, build and budget gates.
