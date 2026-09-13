import { useEffect } from "react";

// Store-driven dialogs have no Radix Trigger. Keep the last workspace control
// across chained dialogs, whose individual inputs may already be unmounted.
let users = 0;
let outside: HTMLElement | null = null;
function remember(event: FocusEvent) {
  const element = event.target;
  if (element instanceof HTMLElement && element !== document.body &&
      element !== document.documentElement &&
      !element.closest('[role="dialog"],[role="alertdialog"],[role="menu"]')) {
    outside = element;
  }
}

export function useDialogFocusReturn(): (event: Event) => void {
  useEffect(() => {
    if (users++ === 0) document.addEventListener("focusin", remember);
    return () => {
      if (--users === 0) {
        document.removeEventListener("focusin", remember);
        outside = null;
      }
    };
  }, []);
  return event => {
    event.preventDefault();
    // A follow-up or parent dialog owns focus until the whole chain closes.
    if (document.querySelector('[role="dialog"],[role="alertdialog"]')) return;
    if (outside?.isConnected) outside.focus();
  };
}
