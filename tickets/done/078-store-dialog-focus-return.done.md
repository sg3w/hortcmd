# TICKET-078: Return focus after store-driven dialog chains

## Problem

Dialogs opened through stores do not have a Radix Trigger. Radix's default close
handler therefore has no trigger to focus. A regression test reproduces a prompt
followed by a confirmation; after the confirmation closes, focus is on the body
instead of the invoking workspace control. Found while verifying ticket 114.
Interpose immediately because keyboard users lose their position after operations.

## Fix

A shared, reference-counted focus listener remembers the last workspace control.
Dialog and popup-menu content do not replace that target. AppDialog and the
operation dialogs restore it once the complete dialog chain has closed; a still
open dialog keeps ownership of focus. Disconnected controls are not focused, and
the listener and target are released when the last consumer unmounts.

## Verification

The chained prompt/confirmation regression failed before the fix and passes after
it. Existing dialog tests continue to pass. Full `npm run check` passed on Node
20.19.0: 387 Rust tests (25 ignored), 417 frontend tests, formatting, Clippy,
generated bindings, TypeScript, translation gates, production build and budgets.
Local commit only; no push.
