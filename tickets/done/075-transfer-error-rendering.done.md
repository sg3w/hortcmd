# TICKET-075: Render structured transfer errors as localized messages

## Problem

`TransferCard` joins `ItemError` objects directly. A failed copy/move therefore
shows `[object Object]` instead of the item name and the localized failure.
Found during ticket 043's operation-announcement audit. Interposed immediately
because users need readable errors before retrying failed file operations.

## Implementation

Use the existing shared `describeItemErrors` renderer before limiting the card to
six lines. Keep backend diagnostic details out of the UI, and recompute wording
when the selected language changes. No backend or transfer behavior changes.

## Validation

A component regression test renders a failed transfer, verifies the affected file
and German message, switches to English and verifies the translated message.
It also rejects object coercion and diagnostic-detail leakage.

Full `npm run check` passed on Node 20.19.0: 387 Rust tests passed (25
ignored), 397 frontend tests passed, and formatting, Clippy, generated bindings,
TypeScript, translation checks and production build/bundle budget passed.
Local commit only; no push.
