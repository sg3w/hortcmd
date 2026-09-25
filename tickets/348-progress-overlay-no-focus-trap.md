# TICKET-348: Add a focus trap to ProgressOverlay, and share dialog boilerplate

Discovered during a full-codebase review (2026-09-22).

## Problem

`ProgressOverlay` in `src/components/ui/OperationDialogs.tsx` (around lines
314-329) is a hand-rolled `fixed inset-0` overlay (not a Radix `Dialog.Root`),
so unlike every sibling dialog in the same file it has no focus trap —
background controls (menu bar, pane rows) stay in the normal Tab order while
it is shown.

While `useOps().busy` is true (a synchronous delete/mkdir in flight), a
keyboard user can Tab past the visually-covering overlay into a background
button (e.g. a menu item) and activate it with Enter/Space, since nothing
marks the rest of the tree `aria-hidden`/`inert` or traps focus the way
Radix's `Dialog.Content` does for every other blocking dialog here.

Related: this file's four simplest dialogs (`CollisionDialog`,
`ConfirmDialog`, `PromptDialog`, `ForceCloseDialog`) also hand-roll nearly
identical `Dialog.Root`/`Portal`/`Overlay`/`Content` boilerplate instead of
the shared `AppDialog` wrapper every other dialog in the codebase uses,
making a future chrome fix (like this one) easy to apply to three of the four
and miss the fourth.

## Required change

- Back `ProgressOverlay` with `Dialog.Root`/`FocusScope` like its siblings
  (even non-dismissible, `modal` mode alone gives the focus trap), or
  explicitly move focus onto the overlay and trap Tab within it while `busy`
  is true.
- While in this file, consider extracting a small `SimpleDialog` primitive
  (fixed-size sibling of `AppDialog`) for `CollisionDialog`/`ConfirmDialog`/
  `PromptDialog`/`ForceCloseDialog` to share, so this class of fix only needs
  applying once in the future.

## Verification

- Manual/RTL test: with `busy` true, Tab from the overlay and assert focus
  does not escape to a background control.
- `npm run check`.
