// ============================================================
// Shared bookkeeping between dialog key handlers and the global
// TC key handler (TICKET-014).
//
// The global handler in `features/commander/useKeyboard.ts` listens on
// `window`, i.e. above the React root, so it sees every keystroke *after*
// React has dispatched it. A dialog that closes on `Enter`/`Escape` unmounts
// during that very dispatch, which means `document.activeElement` has already
// fallen back to `<body>` by the time the global handler runs — the keystroke
// then looks like one typed in the file pane. The event object itself outlives
// the unmount, so it is the reliable marker.
// ============================================================

import type { KeyboardEvent as ReactKeyboardEvent } from "react";

const claimedByDialog = new WeakSet<KeyboardEvent>();

function nativeOf(e: KeyboardEvent | ReactKeyboardEvent): KeyboardEvent {
  return "nativeEvent" in e ? e.nativeEvent : e;
}

/**
 * Marks a keystroke as belonging to a dialog. Attached as
 * `onKeyDownCapture` on every dialog content, so the mark is set before any
 * handler inside the dialog can close it.
 */
export function markDialogKey(e: KeyboardEvent | ReactKeyboardEvent): void {
  claimedByDialog.add(nativeOf(e));
}

/** True when a dialog has already claimed this keystroke. */
export function isDialogKey(e: KeyboardEvent): boolean {
  return claimedByDialog.has(e);
}

/**
 * Consumes a keystroke inside a dialog: suppresses the default action, stops
 * propagation (React forwards this to the native event, so it never reaches
 * the `window` listener) and keeps the dialog mark for the case where the
 * event still gets there.
 */
export function consumeDialogKey(e: KeyboardEvent | ReactKeyboardEvent): void {
  markDialogKey(e);
  e.preventDefault();
  e.stopPropagation();
}
