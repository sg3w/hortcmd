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

## Resolution (2026-09-27)

`ProgressOverlay` is no longer a hand-rolled `fixed inset-0` div. It is a modal
Radix dialog like every sibling in this file, so while `useOps().busy` is true
the rest of the tree is `aria-hidden` and Tab cannot reach a background
control. It is non-dismissible (`onOpenChange` is a no-op — the operation is
synchronous and has no cancel), and `modal` alone supplies the trap. The
`role="status"` live region is kept by rendering it through `Dialog.Title
asChild`, so the announcement is unchanged and Radix still has the title it
needs to label the dialog. The dialog stays mounted with `open={busy}` rather
than being unmounted while open, which would leak `pointer-events: none` onto
`<body>`.

The second half of the ticket is done too: a `SimpleDialog` primitive —
the fixed-size sibling of `AppDialog` — now owns the
`Dialog.Root`/`Portal`/`Overlay`/`Content` chrome, the focus return and the
key-capture that `CollisionDialog`, `ConfirmDialog`, `PromptDialog`,
`ForceCloseDialog` and now `ProgressOverlay` each copied. Callers pass only
their z-pair, width and whether they have a description.

One behaviour had to be made explicit while doing this. `ConfirmDialog` relied
on React's `autoFocus` beating Radix's "focus the first tabbable"; with the
progress dialog now closing in the same commit that opens the error
confirmation, Radix won instead and focus landed on Cancel. The dialog now says
where focus goes via `onOpenAutoFocus` and a ref to the confirm button, which
is deterministic regardless of what else mounts in that commit.

A test focuses a background button, sets `busy`, and asserts the background is
inside an `aria-hidden` subtree, that Tab does not return to it, and that Enter
does not activate it; it fails against the previous plain-div overlay
(verified).

Validation: `npm run check` passed on macOS.
