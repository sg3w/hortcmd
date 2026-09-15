# TICKET-077: Preserve dialogs opened by confirmation callbacks

## Problem

The confirmation button executes `onConfirm` and then clears both operation
dialogs. If the callback opens an error, retry or input prompt synchronously,
that new dialog disappears immediately. Found while verifying nested keyboard
workflows for 114; interposed because it hides follow-up questions and errors.

## Fix

Capture the callback, close the old confirmation, then execute the callback.
This matches the existing prompt-submission order and preserves new dialog state.

## Validation

A component regression test confirms an operation whose callback opens another
prompt. It failed before the fix because the follow-up dialog was absent.
Full `npm run check` passed on Node 20.19.0: 387 Rust tests (25 ignored),
412 frontend tests, formatting, Clippy, generated bindings, TypeScript,
translation checks, production build and bundle budgets. Local commit only;
no push.
